//! "Your orders" (design-ui 2.7): what the player asked of a village, read
//! from the village's own tables and from the orders this ledger sent that
//! have not been applied yet or were refused. Pure: the ledger keeps the
//! sent orders, the simulation keeps the rest.
use super::Action;
use super::economy::TargetStep;
use super::model::{Entry, Extra};
use crate::game_ui::words;
use island_content::village::frontier::oversee::{
    PriorityLevel, PriorityRowView, ProfessionRowView,
};
use npc_framework::prelude::SimId;

/// What an order sent from the ledger asked for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Sent {
    /// A headcount for a job; `None` hands the job back to the village.
    Job {
        key: String,
        display: String,
        wanted: Option<u32>,
    },
    /// A level for a topic, or for one job's workplaces.
    Priority {
        topic: &'static str,
        profession: Option<String>,
        display: String,
        level: PriorityLevel,
    },
}

impl Sent {
    fn key(&self) -> String {
        match self {
            Self::Job { key, .. } => format!("job_{key}"),
            Self::Priority {
                topic, profession, ..
            } => format!(
                "priority_{topic}_{}",
                profession.as_deref().unwrap_or("all")
            ),
        }
    }
}

/// An order this ledger sent: its recorded command, its village, what it
/// asked, and the village's refusal once there is one. View state only.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SentOrder {
    pub(crate) command: (u64, u32),
    pub(crate) village: SimId,
    pub(crate) what: Sent,
    pub(crate) refused: Option<String>,
}

/// How an order stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OrderStatus {
    /// Sent, not applied yet (time is paused, or the next tick takes it).
    Queued,
    /// A headcount the village is still working towards.
    Working,
    /// A headcount the village has reached.
    Met,
    /// A level other than normal.
    Priority,
    /// The village could not do it; stays until dismissed.
    Refused,
}

/// One line of the block: its key, state, words and its button.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct OrderLine {
    pub(crate) key: String,
    pub(crate) status: OrderStatus,
    pub(crate) text: String,
    pub(crate) button: Option<(String, Action)>,
}

/// "Bakers": a job's display name, plural, capitalised.
pub(crate) fn jobs(display: &str) -> String {
    words::capital(&words::noun(display, 2))
}

fn level_word(level: PriorityLevel) -> &'static str {
    match level {
        PriorityLevel::Low => "low",
        PriorityLevel::Normal => "normal",
        PriorityLevel::High => "high",
    }
}

const QUEUED: &str = " Queued: it starts when time runs (Space).";

/// The lines of "Your orders" for `village`: orders still waiting or
/// refused first, then the headcounts the village works towards, then the
/// levels other than normal. `paused` decides whether a waiting order
/// mentions time.
pub(crate) fn order_lines(
    village: SimId,
    jobs_table: &[ProfessionRowView],
    priorities: &[PriorityRowView],
    sent: &[SentOrder],
    paused: bool,
) -> Vec<OrderLine> {
    let mut lines = Vec::new();
    let mine: Vec<&SentOrder> = sent
        .iter()
        .filter(|order| order.village == village)
        .collect();
    let line_key = |what: &Sent| format!("order_{}_{}", village.0, what.key());
    for order in &mine {
        let key = line_key(&order.what);
        let line = match (&order.what, &order.refused) {
            (Sent::Job { display, .. }, Some(reason))
            | (Sent::Priority { display, .. }, Some(reason)) => {
                let name = match &order.what {
                    Sent::Job { .. } => jobs(display),
                    Sent::Priority { .. } => display.clone(),
                };
                OrderLine {
                    key: format!("{key}_refused{}_{}", order.command.0, order.command.1),
                    status: OrderStatus::Refused,
                    text: format!(
                        "{name}: the village could not do that. {}",
                        words::sentence(reason)
                    ),
                    button: Some(("Dismiss".to_owned(), Action::DismissOrder(order.command))),
                }
            }
            (
                Sent::Job {
                    display, wanted, ..
                },
                None,
            ) => OrderLine {
                key,
                status: OrderStatus::Queued,
                text: format!(
                    "{}: {}.{}",
                    jobs(display),
                    wanted.map_or_else(
                        || "the village decides".to_owned(),
                        |wanted| format!("{wanted} wanted")
                    ),
                    if paused { QUEUED } else { "" }
                ),
                button: None,
            },
            (Sent::Priority { display, level, .. }, None) => OrderLine {
                key,
                status: OrderStatus::Queued,
                text: format!(
                    "{display}: {}.{}",
                    level_word(*level),
                    if paused { QUEUED } else { "" }
                ),
                button: None,
            },
        };
        lines.push(line);
    }
    let waiting = |what_key: &str| {
        mine.iter()
            .any(|order| order.refused.is_none() && order.what.key() == what_key)
    };
    for row in jobs_table {
        let Some(wanted) = row.wanted else {
            continue;
        };
        if waiting(&format!("job_{}", row.key)) {
            continue;
        }
        let met = wanted == row.current;
        let reason = row
            .reason
            .as_deref()
            .filter(|reason| plain_reason(reason))
            .map(words::sentence)
            .filter(|reason| !reason.is_empty())
            .unwrap_or_else(|| "The next one changes job after the day's work.".to_owned());
        lines.push(OrderLine {
            key: format!("order_{}_job_{}", village.0, row.key),
            status: if met {
                OrderStatus::Met
            } else {
                OrderStatus::Working
            },
            text: if met {
                format!(
                    "{}: {wanted} wanted, {} now.",
                    jobs(&row.display),
                    row.current
                )
            } else {
                format!(
                    "{}: {wanted} wanted, {} now. {reason}",
                    jobs(&row.display),
                    row.current
                )
            },
            button: Some((
                "Clear".to_owned(),
                Action::Target {
                    settlement: village,
                    profession: row.key.clone(),
                    step: TargetStep::VillageDecides,
                },
            )),
        });
    }
    for row in priorities {
        if row.level == PriorityLevel::Normal {
            continue;
        }
        let key = format!(
            "priority_{}_{}",
            row.topic,
            row.profession.as_deref().unwrap_or("all")
        );
        if waiting(&key) {
            continue;
        }
        lines.push(OrderLine {
            key: format!("order_{}_{key}", village.0),
            status: OrderStatus::Priority,
            text: priority_text(&row.display, row.level, &row.effect),
            button: Some((
                "Normal".to_owned(),
                Action::Priority {
                    settlement: village,
                    topic: row.topic,
                    profession: row.profession.clone(),
                    level: PriorityLevel::Normal,
                },
            )),
        });
    }
    lines
}

/// A level in force: "Housing: high. Homes are built sooner." When the
/// simulation's effect already starts with the level ("High: …"), it is
/// not said twice.
pub(crate) fn priority_text(topic: &str, level: PriorityLevel, effect: &str) -> String {
    let word = level_word(level);
    let trimmed = effect.trim();
    let rest = trimmed
        .get(..word.len() + 1)
        .filter(|head| head.eq_ignore_ascii_case(&format!("{word}:")))
        .map_or(trimmed, |_| trimmed[word.len() + 1..].trim_start());
    format!("{topic}: {word}. {}", words::sentence(rest))
}

/// Whether the simulation's reason reads as player words. Its old Jobs
/// sentence broke a line inside the string, leaving a run of spaces; that
/// one falls back to the view's own sentence (the sim now writes plain
/// singular and plural).
pub(crate) fn plain_reason(reason: &str) -> bool {
    !reason.contains("  ")
}

/// The block's rows: a head and one row per line; nothing without orders.
pub(crate) fn order_entries(village: SimId, name: &str, lines: Vec<OrderLine>) -> Vec<Entry> {
    if lines.is_empty() {
        return Vec::new();
    }
    let mut rows = vec![Entry::strong(
        &format!("orders_{}", village.0),
        format!("Your orders in {name}"),
    )];
    for line in lines {
        rows.push(
            Entry::text(&line.key, line.text, false).with(Extra::Order(line.status, line.button)),
        );
    }
    rows
}

/// The second line of a resident's world tag after an ordered job change:
/// "Now a baker · your order".
// For the world's marks (`village_world`, `island_atmosphere`), which the
// world stream places; until then only the tests call these.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn order_tag(job: &str) -> String {
    let job = words::noun(job, 1);
    let article = if job.starts_with(['a', 'e', 'i', 'o', 'u']) {
        "an"
    } else {
        "a"
    };
    format!("Now {article} {job} \u{b7} your order")
}

/// The pill over a workplace an unmet order waits on: "1 more baker
/// wanted", "2 more bakers wanted".
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn order_pill(job: &str, missing: u32) -> String {
    format!(
        "{missing} more {} wanted",
        words::noun(job, u64::from(missing))
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn job(key: &str, display: &str, current: u32, wanted: Option<u32>) -> ProfessionRowView {
        ProfessionRowView {
            key: key.into(),
            display: display.into(),
            current,
            wanted,
            free_seats: 1,
            adjustable: true,
            reason: None,
            cause: None,
        }
    }

    #[test]
    fn your_orders_lists_queued_working_met_and_refused() {
        let village = SimId(3);
        let table = [
            job("baker", "Baker", 2, Some(3)),
            job("woodcutter", "Woodcutter", 4, Some(4)),
            job("potter", "Potter", 0, None),
        ];
        let priorities = [
            PriorityRowView {
                topic: "food_reserve",
                display: "Food reserve".into(),
                profession: None,
                level: PriorityLevel::High,
                effect: "Harbour sees to it first".into(),
            },
            PriorityRowView {
                topic: "housing",
                display: "Housing".into(),
                profession: None,
                level: PriorityLevel::Normal,
                effect: String::new(),
            },
        ];
        let sent = [
            SentOrder {
                command: (10, 1),
                village,
                what: Sent::Job {
                    key: "potter".into(),
                    display: "Potter".into(),
                    wanted: Some(2),
                },
                refused: None,
            },
            SentOrder {
                command: (10, 2),
                village,
                what: Sent::Job {
                    key: "innkeeper".into(),
                    display: "Innkeeper".into(),
                    wanted: Some(1),
                },
                refused: Some("an inn fills its own places".into()),
            },
            SentOrder {
                command: (10, 3),
                village: SimId(9),
                what: Sent::Job {
                    key: "potter".into(),
                    display: "Potter".into(),
                    wanted: Some(5),
                },
                refused: None,
            },
        ];
        let lines = order_lines(village, &table, &priorities, &sent, true);
        let said: Vec<(OrderStatus, &str)> = lines
            .iter()
            .map(|line| (line.status, line.text.as_str()))
            .collect();
        assert_eq!(
            said,
            vec![
                (
                    OrderStatus::Queued,
                    "Potters: 2 wanted. Queued: it starts when time runs (Space)."
                ),
                (
                    OrderStatus::Refused,
                    "Innkeepers: the village could not do that. An inn fills its own places."
                ),
                (
                    OrderStatus::Working,
                    "Bakers: 3 wanted, 2 now. The next one changes job after the day's work."
                ),
                (OrderStatus::Met, "Woodcutters: 4 wanted, 4 now."),
                (
                    OrderStatus::Priority,
                    "Food reserve: high. Harbour sees to it first."
                ),
            ]
        );
        assert_eq!(
            lines[1].button,
            Some(("Dismiss".to_owned(), Action::DismissOrder((10, 2))))
        );
        assert_eq!(lines[2].button.as_ref().unwrap().0, "Clear");
        assert_eq!(lines[4].button.as_ref().unwrap().0, "Normal");
        // While time runs a waiting order says nothing about time.
        let running = order_lines(village, &table, &priorities, &sent, false);
        assert_eq!(running[0].text, "Potters: 2 wanted.");
        // No orders: no block.
        assert!(order_entries(village, "Harbour", Vec::new()).is_empty());
        let rows = order_entries(village, "Harbour", lines);
        assert_eq!(rows[0].text, "Your orders in Harbour");
    }

    #[test]
    fn a_level_and_a_reason_are_said_once_in_player_words() {
        assert_eq!(
            priority_text(
                "Housing",
                PriorityLevel::High,
                "High: the planner builds homes sooner"
            ),
            "Housing: high. The planner builds homes sooner."
        );
        assert_eq!(
            priority_text(
                "Food reserve",
                PriorityLevel::High,
                "Harbour sees to it first"
            ),
            "Food reserve: high. Harbour sees to it first."
        );
        assert!(!plain_reason(
            "1 more farmer(s) wanted: residents move at their next review, once their              switching rest is over"
        ));
        assert!(plain_reason("no free place in the village"));
        assert!(plain_reason("1 more baker wanted"));
        let mut row = job("baker", "Baker", 2, Some(3));
        row.reason = Some("1 more baker(s) wanted:    at their next review".into());
        let lines = order_lines(SimId(1), &[row], &[], &[], false);
        assert_eq!(
            lines[0].text,
            "Bakers: 3 wanted, 2 now. The next one changes job after the day's work."
        );
    }

    #[test]
    fn an_ordered_job_change_reads_as_you_asked() {
        assert_eq!(order_tag("Baker"), "Now a baker \u{b7} your order");
        assert_eq!(order_tag("innkeeper"), "Now an innkeeper \u{b7} your order");
        assert_eq!(order_pill("baker", 1), "1 more baker wanted");
        assert_eq!(order_pill("Baker", 2), "2 more bakers wanted");
        assert_eq!(
            crate::island_alerts::model::as_you_asked("Tom became a baker.", true),
            "Tom became a baker, as you asked."
        );
        assert_eq!(
            crate::island_alerts::model::as_you_asked("Tom became a baker.", false),
            "Tom became a baker."
        );
        // The journal's notes of a headcount and a priority.
        let note = |effect: &str, payload: serde_json::Value| {
            let read = |key: &str| payload.get(key).cloned();
            crate::island_alerts::model::classify(effect, "A resident", &read).map(|(_, text)| text)
        };
        assert_eq!(
            note(
                "LivingProfessionTargetSet",
                serde_json::json!({"settlement_name": "Harbour", "trade": "baker",
                    "wanted": 3, "by_order": true})
            )
            .as_deref(),
            Some("Harbour wants 3 bakers, as you asked")
        );
        assert_eq!(
            note(
                "LivingVillagePrioritySet",
                serde_json::json!({"settlement_name": "Harbour", "topic": "food_reserve",
                    "level": "high", "by_order": true})
            )
            .as_deref(),
            Some("Harbour sees to food first, as you asked")
        );
        assert_eq!(
            note(
                "RegionalOccupationChanged",
                serde_json::json!({"from": "farmer", "to": "baker", "by_order": true})
            )
            .as_deref(),
            Some("A resident changed from farmer to baker, as you asked")
        );
        // The resident's card says it too.
        assert_eq!(
            crate::resident_inspector::model::job_line("Baker", Some("Harbour"), true),
            "Baker \u{b7} Harbour \u{b7} as you asked"
        );
        assert_eq!(
            crate::resident_inspector::model::job_line("Baker", None, false),
            "Baker"
        );
    }
}
