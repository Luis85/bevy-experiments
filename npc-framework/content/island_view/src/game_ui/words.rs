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
}
