//! One way to say a count of a good: "1 log", "2 logs", "3 bread". Every
//! count-and-good string of the island interface goes through [`count`], so
//! no card says "Order 2 log" and another "1 planks".

/// Goods counted by weight or measure, never with a plural "s".
const MASS: [&str; 34] = [
    "bread", "cheese", "clay", "cloth", "clothes", "coal", "compost", "fiber", "fibre", "firewood",
    "fish", "flax", "flour", "food", "fruit", "grain", "hay", "honey", "iron", "leather", "manure",
    "meat", "milk", "ore", "porridge", "salt", "sand", "soup", "stew", "stone", "thread",
    "venison", "water", "wool",
];

/// A good's key or display name as words: "village_clean_water" reads
/// "clean water".
fn words(kind: &str) -> String {
    kind.strip_prefix("village_")
        .unwrap_or(kind)
        .replace('_', " ")
        .to_lowercase()
}

fn is_mass(last: &str) -> bool {
    MASS.contains(&last) || last == "wood"
}

fn singular(word: &str) -> String {
    if let Some(stem) = word.strip_suffix("ies") {
        format!("{stem}y")
    } else if word.ends_with("ss") || !word.ends_with('s') {
        word.to_owned()
    } else {
        word[..word.len() - 1].to_owned()
    }
}

fn plural(word: &str) -> String {
    let vowel_before_y = word
        .strip_suffix('y')
        .and_then(|stem| stem.chars().last())
        .is_some_and(|c| "aeiou".contains(c));
    if word.ends_with('s') && !word.ends_with("ss") {
        word.to_owned()
    } else if word.ends_with('y') && !vowel_before_y {
        format!("{}ies", &word[..word.len() - 1])
    } else if ["ss", "sh", "ch", "x"]
        .iter()
        .any(|end| word.ends_with(end))
    {
        format!("{word}es")
    } else {
        format!("{word}s")
    }
}

/// The good's noun for `count` of it, lower case: `noun("log", 2)` is
/// "logs", `noun("planks", 1)` is "plank", `noun("bread", 3)` is "bread".
/// Takes a key ("village_log") or a display name ("logs").
pub fn noun(kind: &str, count: u64) -> String {
    let text = words(kind);
    let (head, last) = match text.rsplit_once(' ') {
        Some((head, last)) => (format!("{head} "), last.to_owned()),
        None => (String::new(), text.clone()),
    };
    if is_mass(&last) || is_mass(&singular(&last)) {
        return text;
    }
    let last = if count == 1 {
        singular(&last)
    } else {
        plural(&singular(&last))
    };
    format!("{head}{last}")
}

/// "1 log", "2 logs", "3 bread".
pub fn count(count: u64, kind: &str) -> String {
    format!("{count} {}", noun(kind, count))
}

/// The calendar day index of `tick`, the one the top bar's date counts.
fn day_index(world: &bevy::prelude::World, tick: u64) -> u64 {
    let config = world
        .get_resource::<npc_framework::prelude::SimConfig>()
        .copied()
        .unwrap_or_default();
    island_content::village::calendar_at(tick, &config, true).0
}

/// When `tick` was, in the one calendar the player sees: "today",
/// "yesterday", else "Autumn, day 4", with ", year 2" from the second
/// year on. Lower case where it is relative; capitalise at the start of a
/// line.
pub fn when(world: &bevy::prelude::World, tick: u64) -> String {
    when_day(world, day_index(world, tick))
}

/// [`when`] for a calendar day index (`calendar_at(..).0`).
pub fn when_day(world: &bevy::prelude::World, day: u64) -> String {
    let now = world
        .get_resource::<npc_framework::prelude::SimClock>()
        .map_or(0, |clock| clock.tick);
    let today = day_index(world, now);
    date_words(day, today, island_content::village::DAYS_PER_SEASON)
}

/// The words of [`when_day`] without a world: `day` and `today` are
/// calendar day indices, `season_days` the length of a season.
pub fn date_words(day: u64, today: u64, season_days: u64) -> String {
    if day == today {
        return "today".to_owned();
    }
    if day + 1 == today {
        return "yesterday".to_owned();
    }
    calendar_words(day, season_days)
}

/// The date of `tick` as the top bar says it, never relative: "Autumn,
/// day 6" (", year 2" from the second year on).
pub fn date(world: &bevy::prelude::World, tick: u64) -> String {
    calendar_words(
        day_index(world, tick),
        island_content::village::DAYS_PER_SEASON,
    )
}

/// "Autumn, day 6" for calendar day index `day`.
pub fn calendar_words(day: u64, season_days: u64) -> String {
    const SEASONS: [&str; 4] = ["Spring", "Summer", "Autumn", "Winter"];
    let length = season_days.max(1);
    let season = SEASONS[(day / length % 4) as usize];
    let date = format!("{season}, day {}", day % length + 1);
    match day / (4 * length) + 1 {
        1 => date,
        year => format!("{date}, year {year}"),
    }
}

/// A moment as the 24-hour face of the top bar shows it, the day starting
/// at 06:00: `permille` is the share of the day gone.
pub fn clock_face(permille: u32) -> String {
    let minutes = (6 * 60 + permille.min(999) * 24 * 60 / 1000) % (24 * 60);
    format!("{:02}:{:02}", minutes / 60, minutes % 60)
}

/// [`when`] with the time for a moment near now: "today, 09:10",
/// "yesterday, 18:40", else the calendar form.
pub fn moment(world: &bevy::prelude::World, tick: u64) -> String {
    let config = world
        .get_resource::<npc_framework::prelude::SimConfig>()
        .copied()
        .unwrap_or_default();
    let when = when(world, tick);
    if when == "today" || when == "yesterday" {
        format!("{when}, {}", clock_face(config.time_of_day_permille(tick)))
    } else {
        when
    }
}

/// "today" as "Today": the first letter in capitals.
pub fn capital(text: &str) -> String {
    let mut letters = text.chars();
    letters.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(letters).collect()
    })
}

/// Whether time is paused now (no `SimControl` counts as paused). Words
/// about time appear only then; while time runs no sentence mentions it.
pub fn paused(world: &bevy::prelude::World) -> bool {
    world
        .get_resource::<npc_framework::prelude::SimControl>()
        .is_none_or(npc_framework::prelude::SimControl::is_stopped)
}

/// The ledger's foot after an order, by whether time runs.
pub fn order_given(paused: bool) -> &'static str {
    if paused {
        "Order queued. It starts when time runs (Space)."
    } else {
        "Order given."
    }
}

/// A refusal of the village in one sentence: "The village could not do
/// that. {Reason}."
pub fn village_refused(reason: &str) -> String {
    format!("The village could not do that. {}", sentence(reason))
}

/// The simulation's words as a sentence: capital first, full stop last.
pub fn sentence(reason: &str) -> String {
    let mut letters = reason.trim().chars();
    let mut out: String = letters.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(letters).collect()
    });
    if !out.is_empty() && !out.ends_with(['.', '!', '?']) {
        out.push('.');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_agree_with_their_goods() {
        assert_eq!(count(2, "log"), "2 logs");
        assert_eq!(count(3, "plank"), "3 planks");
        assert_eq!(count(1, "planks"), "1 plank");
        assert_eq!(count(1, "logs"), "1 log");
        assert_eq!(count(2, "village_log"), "2 logs");
        assert_eq!(count(3, "bread"), "3 bread");
        assert_eq!(count(1, "ore"), "1 ore");
        assert_eq!(count(2, "axe"), "2 axes");
        assert_eq!(count(1, "axes"), "1 axe");
        assert_eq!(count(2, "pickaxe"), "2 pickaxes");
        assert_eq!(count(2, "berry"), "2 berries");
        assert_eq!(count(1, "berries"), "1 berry");
        assert_eq!(count(2, "village_clean_water"), "2 clean water");
        assert_eq!(count(4, "hide"), "4 hides");
        assert_eq!(count(1, "ingots"), "1 ingot");
        assert_eq!(count(5, "Fiber"), "5 fiber");
        assert_eq!(count(2, "warm clothes"), "2 warm clothes");
        assert_eq!(count(0, "log"), "0 logs");
    }

    #[test]
    fn a_date_is_relative_near_now_and_in_the_season_otherwise() {
        // Seven-day seasons; day index 19 is the sixth day of autumn.
        assert_eq!(date_words(19, 19, 7), "today");
        assert_eq!(date_words(18, 19, 7), "yesterday");
        assert_eq!(date_words(17, 19, 7), "Autumn, day 4");
        assert_eq!(calendar_words(19, 7), "Autumn, day 6");
        assert_eq!(calendar_words(21, 7), "Winter, day 1");
        assert_eq!(calendar_words(28, 7), "Spring, day 1, year 2");
        assert_eq!(capital("today"), "Today");
    }
}
