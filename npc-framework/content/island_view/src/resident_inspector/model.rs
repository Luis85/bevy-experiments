//! Pure projections for the resident inspector: need levels, trait words, the
//! day's schedule and the "Why" text built from a decision record. Nothing
//! here reads a `World`; the widget layer supplies plain values.
#[cfg(test)]
use island_content::village::resident::ScheduleSpec;
use island_content::village::resident::{BlockKind, DecisionView, ResidentTraits};
use island_content::village::view::Repose;
use serde_json::Value;

/// How pressing a need is, for the bars and the roster's indicator.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum NeedLevel {
    #[default]
    Fine,
    /// Below the level at which the resident acts on it.
    Low,
    /// Below the level at which it interrupts everything else.
    Critical,
}

/// `value`, `low` and `critical` are milli-units (0..=100000, higher is better).
pub(crate) fn need_level(value: i32, low: i32, critical: i32) -> NeedLevel {
    if value < critical {
        NeedLevel::Critical
    } else if value < low {
        NeedLevel::Low
    } else {
        NeedLevel::Fine
    }
}

/// Whole percent of a milli-unit need, never above 100 or below 0.
/// The badge: "Doing fine", "In need: warmth" or "In danger: food", from the
/// most pressing need. Colour never speaks alone: the caller adds the shape.
pub(crate) fn status_words(
    bars: &[(&'static str, NeedLevel)],
) -> (crate::game_ui::tokens::Status, String) {
    use crate::game_ui::tokens::Status;
    let worst = |level| {
        bars.iter()
            .find(|(_, found)| *found == level)
            .map(|(label, _)| label.to_lowercase())
    };
    if let Some(need) = worst(NeedLevel::Critical) {
        (Status::Crit, format!("In danger: {need}"))
    } else if let Some(need) = worst(NeedLevel::Low) {
        (Status::Warn, format!("In need: {need}"))
    } else {
        (Status::Good, "Doing fine".to_owned())
    }
}

/// One sentence behind the badge.
pub(crate) fn status_sentence(status: crate::game_ui::tokens::Status) -> &'static str {
    use crate::game_ui::tokens::Status;
    match status {
        Status::Crit => "A need is so low that they drop everything else for it.",
        Status::Warn => "A need is low: they will see to it before other work.",
        Status::Idle => "Their story has ended.",
        _ => "No need is pressing.",
    }
}

/// One sentence for a need bar's tooltip.
pub(crate) fn need_sentence(level: NeedLevel) -> &'static str {
    match level {
        NeedLevel::Fine => "Fine.",
        NeedLevel::Low => "Low: they will see to it before other work.",
        NeedLevel::Critical => "In danger: they drop everything else for it.",
    }
}

/// "Stocking up on provisions: nothing they want is for sale." as a title and
/// its reason, split at the first colon.
pub(crate) fn title_and_reason(line: &str) -> (String, Option<String>) {
    match line.split_once(": ") {
        Some((title, reason)) if !title.is_empty() && !reason.is_empty() => {
            (capitalise(title), Some(capitalise(reason)))
        }
        _ => (capitalise(line), None),
    }
}

/// "Checked first: not thirsty, not hungry, no meal due." as chips that name
/// what was checked: "Thirst", "Hunger", "Meal".
pub(crate) fn checked_chips(ruled_out: &str) -> Vec<String> {
    let list = ruled_out
        .split_once(": ")
        .map_or(ruled_out, |(_, list)| list)
        .trim_end_matches('.');
    list.split(", ")
        .filter(|part| !part.is_empty())
        .map(|part| {
            match part.trim() {
                "not thirsty" => "Thirst",
                "not hungry" => "Hunger",
                "not tired" => "Tiredness",
                "not cold" => "Cold",
                "not bedtime yet" => "Bedtime",
                "no meal due" => "Meal",
                "no free time now" => "Free time",
                "not night" => "Night",
                other => {
                    return capitalise(other.trim_start_matches("not ").trim_start_matches("no "));
                }
            }
            .to_owned()
        })
        .collect()
}

pub(crate) fn percent_of(value: i32) -> i32 {
    (value / 1000).clamp(0, 100)
}

/// The four traits as short words, in a fixed order: hours, work, company,
/// money. `sociable_min` is the world's own threshold for taking free time.
pub(crate) fn trait_words(traits: ResidentTraits, sociable_min: i32) -> [&'static str; 4] {
    let hours = match traits.chronotype {
        value if value <= -34 => "early riser",
        value if value >= 34 => "night owl",
        _ => "keeps regular hours",
    };
    let work = match traits.diligence {
        value if value >= 67 => "hard-working",
        value if value < 40 => "easy-going",
        _ => "steady worker",
    };
    let company = match traits.sociability {
        value if value < sociable_min => "keeps to themself",
        value if value >= 67 => "sociable",
        _ => "friendly",
    };
    let money = match traits.thrift {
        value if value >= 67 => "thrifty",
        value if value < 34 => "free-spending",
        _ => "careful with money",
    };
    [hours, work, company, money]
}

/// What a moment of the day is for, as the strip shows it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Slot {
    /// No block covers it.
    Free,
    Work,
    Meal,
    Leisure,
    Sleep,
}

impl Slot {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Free => "own time",
            Self::Work => "work",
            Self::Meal => "meal",
            Self::Leisure => "free time",
            Self::Sleep => "sleep",
        }
    }
}

/// One run of the day, `from` inclusive and `to` exclusive, in permille of the
/// day (0 is sunrise).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Segment {
    pub(crate) slot: Slot,
    pub(crate) from: i32,
    pub(crate) to: i32,
}

pub(crate) fn contains(from: i32, to: i32, at: i32) -> bool {
    if from <= to {
        from <= at && at < to
    } else {
        at >= from || at < to
    }
}

/// Whether a resident with `traits` is inside a block of `kind` at `permille`
/// of the day. The same rule as the simulation's `resident::in_block`: every
/// block moves with the chronotype, and leisure starts later for the diligent.
/// The panel reads the simulation's own blocks (`plan_of_blocks`); this is
/// the reference the tests hold them against.
#[cfg(test)]
pub(crate) fn in_slot(
    spec: &ScheduleSpec,
    traits: ResidentTraits,
    kind: BlockKind,
    permille: i32,
) -> bool {
    let at = (permille - traits.chronotype * spec.chronotype_shift / 100).rem_euclid(1000);
    spec.blocks
        .iter()
        .filter(|block| block.kind == kind)
        .any(|block| {
            if kind != BlockKind::Leisure {
                return contains(block.from, block.to, at);
            }
            let length = if block.to > block.from {
                block.to - block.from
            } else {
                block.to + 1000 - block.from
            };
            let delay = (traits.diligence - 50) * spec.diligence_shift / 50;
            delay < length && contains((block.from + delay).rem_euclid(1000), block.to, at)
        })
}

#[cfg(test)]
fn slot_of(spec: &ScheduleSpec, traits: ResidentTraits, permille: i32) -> Slot {
    // Blocks overlap: the one the resident's tree serves first shows
    // (sleep, then a meal, then leisure, then work).
    for (kind, slot) in [
        (BlockKind::Sleep, Slot::Sleep),
        (BlockKind::Meal, Slot::Meal),
        (BlockKind::Leisure, Slot::Leisure),
        (BlockKind::Work, Slot::Work),
    ] {
        if in_slot(spec, traits, kind, permille) {
            return slot;
        }
    }
    Slot::Free
}

/// The resident's own day from sunrise to sunrise, as runs of one slot each,
/// from the simulation's effective blocks (`view::schedule_blocks`): each is
/// `(kind, from, to)` in permille of the day, already moved by the resident's
/// own hours; `from > to` wraps past sunrise.
pub(crate) fn plan_of_blocks(blocks: &[(BlockKind, i32, i32)]) -> Vec<Segment> {
    let mut plan: Vec<Segment> = Vec::new();
    for permille in 0..1000 {
        // Blocks overlap: the one the resident's tree serves first shows
        // (sleep, then a meal, then leisure, then work), as the simulation
        // decides.
        let slot = [
            (BlockKind::Sleep, Slot::Sleep),
            (BlockKind::Meal, Slot::Meal),
            (BlockKind::Leisure, Slot::Leisure),
            (BlockKind::Work, Slot::Work),
        ]
        .into_iter()
        .find(|(kind, _)| {
            blocks
                .iter()
                .any(|(of, from, to)| of == kind && contains(*from, *to, permille))
        })
        .map_or(Slot::Free, |(_, slot)| slot);
        match plan.last_mut() {
            Some(last) if last.slot == slot => last.to = permille + 1,
            _ => plan.push(Segment {
                slot,
                from: permille,
                to: permille + 1,
            }),
        }
    }
    plan
}

/// The simulation's decision view in the shape of its journal record, which
/// [`why`] reads: one description of a choice, whichever way it arrived.
pub(crate) fn record_of(view: &DecisionView) -> Value {
    let considered: Vec<Value> = view
        .considered
        .iter()
        .map(|option| {
            let mut entry = serde_json::Map::new();
            entry.insert("node".into(), Value::from(option.node.clone()));
            if let Some(score) = option.score {
                entry.insert("score".into(), Value::from(score));
                entry.insert("chosen".into(), Value::from(option.chosen));
                return Value::Object(entry);
            }
            let rejected = match (&option.condition, option.does_not_apply, &option.reason) {
                (Some(condition), true, _) => format!("{condition} is false"),
                (Some(condition), false, Some(reason)) => {
                    format!("{condition} holds, but {reason}")
                }
                (None, _, Some(reason)) => reason.clone(),
                (Some(condition), false, None) => {
                    format!("{condition} holds, but it could not start")
                }
                (None, _, None) => "it could not start".to_owned(),
            };
            entry.insert("rejected".into(), Value::from(rejected));
            Value::Object(entry)
        })
        .collect();
    let mut record = serde_json::Map::new();
    record.insert("tree".into(), Value::from(view.tree.clone()));
    record.insert("branch".into(), Value::from(view.branch.join(" > ")));
    record.insert("node".into(), Value::from(view.node));
    record.insert("action".into(), Value::from(view.action.clone()));
    record.insert("param".into(), Value::from(view.param.clone()));
    record.insert("activity".into(), Value::from(view.activity));
    record.insert("considered".into(), Value::from(considered));
    // The plan or the craft candidates sit beside the other fields.
    if let Some(Value::Object(extra)) = &view.detail {
        for (key, value) in extra {
            record.insert(key.clone(), value.clone());
        }
    }
    Value::Object(record)
}

/// Who a resident belongs to, for the Overview: what the census says about
/// them and their family, a fact per line. Names come with ages for children.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Family {
    /// The simulation's own age words ("Adult · 34"); before stage and years.
    pub(crate) label: String,
    pub(crate) stage: String,
    pub(crate) age_years: u64,
    pub(crate) partner: Option<String>,
    pub(crate) courting: Option<String>,
    pub(crate) children: Vec<String>,
    pub(crate) parents: Vec<String>,
}

/// The Friends list: `(name, tier, played)` strongest first, five at most,
/// as "Ada · Friend"; the played character is marked "(you)". The simulation
/// lists bonds above nothing; those below
/// the first named tier read "Met" (the data's word there is "stranger").
pub(crate) fn friend_lines(relations: &[(String, String, bool)]) -> Vec<String> {
    relations
        .iter()
        .filter(|(name, _, _)| !name.is_empty())
        .take(5)
        .map(|(name, tier, played)| {
            let tier = if tier == "stranger" { "met" } else { tier };
            format!(
                "{name}{} \u{b7} {}",
                if *played { " (you)" } else { "" },
                capitalise(tier)
            )
        })
        .collect()
}

pub(crate) fn family_lines(family: &Family) -> Vec<String> {
    let mut lines = Vec::new();
    if !family.label.is_empty() {
        lines.push(family.label.clone());
    } else if !family.stage.is_empty() {
        lines.push(format!(
            "{} \u{b7} {} year{} old",
            capitalise(&family.stage),
            family.age_years,
            if family.age_years == 1 { "" } else { "s" }
        ));
    }
    if let Some(partner) = &family.partner {
        lines.push(format!("Partner: {partner}"));
    } else if let Some(courting) = &family.courting {
        lines.push(format!("Courting: {courting}"));
    }
    if !family.children.is_empty() {
        lines.push(format!("Children: {}", family.children.join(", ")));
    }
    if !family.parents.is_empty() {
        lines.push(format!("Parents: {}", family.parents.join(", ")));
    }
    lines
}

/// "Bed 2 of 4 in Harbour cottage 1 · asleep": where they sleep and whether
/// they are in it now. `bed` counts from 0, as the simulation does.
pub(crate) fn bed_line(home: &str, bed: u32, beds: u32, repose: Repose) -> String {
    let place = format!("Bed {} of {beds} in {home}", bed + 1);
    match repose_word(repose) {
        Some(word) => format!("{place} \u{b7} {word}"),
        None => place,
    }
}

/// What the island says when the inspected resident is no longer in the
/// world: the simulation removes a body a day after a death.
pub(crate) fn gone_sentence(name: Option<&str>) -> String {
    match name {
        Some(name) => {
            format!("{name} has been laid to rest. They are remembered on the People page.")
        }
        None => "They are no longer on the island.".to_owned(),
    }
}

/// Where a planned purchase will be made: "They will buy bread at Harbour
/// market stall 1." Without a seller the simulation found none they can pay.
pub(crate) fn purchase_sentence(good: &str, seller: Option<&str>) -> String {
    let good = good.to_lowercase();
    match seller {
        Some(seller) => format!("They will buy {good} at {seller}."),
        None => format!("Nobody offers {good} at a price they can pay right now."),
    }
}

/// Asleep or resting, in a word; nothing for somebody who is up.
pub(crate) fn repose_word(repose: Repose) -> Option<&'static str> {
    match repose {
        Repose::Asleep => Some("asleep"),
        Repose::Resting => Some("resting"),
        Repose::Awake => None,
    }
}

/// The reference for [`plan_of_blocks`]: the same day from the world's
/// schedule and the resident's traits.
#[cfg(test)]
pub(crate) fn day_plan(spec: &ScheduleSpec, traits: ResidentTraits) -> Vec<Segment> {
    let mut plan: Vec<Segment> = Vec::new();
    for permille in 0..1000 {
        let slot = slot_of(spec, traits, permille);
        match plan.last_mut() {
            Some(last) if last.slot == slot => last.to = permille + 1,
            _ => plan.push(Segment {
                slot,
                from: permille,
                to: permille + 1,
            }),
        }
    }
    plan
}

/// Index of the segment that holds `permille`.
pub(crate) fn current_segment(plan: &[Segment], permille: i32) -> Option<usize> {
    let at = permille.rem_euclid(1000);
    plan.iter()
        .position(|segment| segment.from <= at && at < segment.to)
}

/// Clock text of a moment of the day. Sunrise is 06:00, as on the top bar.
pub(crate) fn clock_text(permille: i32) -> String {
    let minutes = (6 * 60 + permille.rem_euclid(1000) * 24 * 60 / 1000) % (24 * 60);
    format!("{:02}:{:02}", minutes / 60, minutes % 60)
}

/// Whether the resident has sat down to today's meal (the simulation's one
/// sit-down meal a day), from their own meal blocks `(from, to)` in day
/// permille and the time now: "Has eaten today.", "Not eaten yet today. Meal
/// at 20:24.", "Not eaten yet today. It is meal time." or "Missed today's
/// meal." `None` when the day has no meal block.
pub(crate) fn meal_line(eaten_today: bool, meals: &[(i32, i32)], permille: i32) -> Option<String> {
    if meals.is_empty() {
        return None;
    }
    if eaten_today {
        return Some("Has eaten today.".to_owned());
    }
    let inside = |(from, to): (i32, i32)| {
        if from <= to {
            from <= permille && permille < to
        } else {
            permille >= from || permille < to
        }
    };
    if meals.iter().copied().any(inside) {
        return Some("Not eaten yet today. It is meal time.".to_owned());
    }
    // A block that starts later today is still to come; the day (as the
    // simulation counts it) ends at sunrise, so a block that wraps past it
    // is never behind.
    let next = meals
        .iter()
        .filter(|(from, _)| *from > permille)
        .map(|(from, _)| *from)
        .min();
    Some(match next {
        Some(from) => format!("Not eaten yet today. Meal at {}.", clock_text(from)),
        None => "Missed today's meal.".to_owned(),
    })
}

/// "Work now · meal 12:00 · free time 21:21 · sleep 22:48": the current slot
/// and the next three changes, wrapping past sunrise.
pub(crate) fn schedule_caption(plan: &[Segment], permille: i32) -> String {
    let Some(at) = current_segment(plan, permille) else {
        return String::new();
    };
    let mut parts = vec![format!("{} now", capitalise(plan[at].slot.label()))];
    let mut last = plan[at].slot;
    for step in 1..plan.len() {
        let segment = plan[(at + step) % plan.len()];
        // A run that wraps past sunrise is one stretch, not two.
        if segment.slot == last {
            continue;
        }
        last = segment.slot;
        parts.push(format!(
            "{} {}",
            segment.slot.label(),
            clock_text(segment.from)
        ));
        if parts.len() == 4 {
            break;
        }
    }
    parts.join(" · ")
}

/// The card's job line: "Farmer · Harbour", and "· as you asked" when the
/// player's order gave them the job.
pub(crate) fn job_line(job: &str, village: Option<&str>, ordered: bool) -> String {
    let mut line = match village {
        Some(village) => format!("{job} \u{b7} {village}"),
        None => job.to_owned(),
    };
    if ordered {
        line.push_str(" \u{b7} as you asked");
    }
    line
}

pub(crate) fn capitalise(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// A production plan in display words.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PlanView {
    pub(crate) goal: String,
    /// The coming steps, first one first; may be shorter than `remaining`.
    pub(crate) steps: Vec<String>,
    pub(crate) remaining: usize,
}

/// What the builder may know besides the record.
pub(crate) struct WhyContext<'a> {
    /// Display name of a good or a recipe key.
    pub(crate) names: &'a dyn Fn(&str) -> String,
    pub(crate) hunter: bool,
    pub(crate) thirst: NeedLevel,
    pub(crate) hunger: NeedLevel,
    pub(crate) warmth: NeedLevel,
    /// The record that began the activity a "carry on" leaf is finishing.
    pub(crate) origin: Option<&'a Value>,
    /// The plan the simulation holds now; wins over the record's own.
    pub(crate) plan: Option<PlanView>,
    /// What the card says they are doing now, where that is more exact than
    /// the leaf's own label ("gathering fiber" for "working to a plan").
    pub(crate) activity: Option<String>,
}

impl WhyContext<'_> {
    #[cfg(test)]
    pub(crate) fn plain(names: &dyn Fn(&str) -> String) -> WhyContext<'_> {
        WhyContext {
            names,
            hunter: false,
            thirst: NeedLevel::Fine,
            hunger: NeedLevel::Fine,
            warmth: NeedLevel::Fine,
            origin: None,
            plan: None,
            activity: None,
        }
    }
}

/// The inspector's "Why" section.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Why {
    /// What they chose and what made them choose it.
    pub(crate) chosen: String,
    /// Up to three things they would have done first, each with why not.
    pub(crate) alternatives: Vec<String>,
    /// Needs that were checked first and found quiet.
    pub(crate) ruled_out: Option<String>,
    pub(crate) plan: Option<String>,
}

impl Why {
    /// The four parts as one text: what the tests read.
    #[cfg(test)]
    pub(crate) fn text(&self) -> String {
        let mut lines = vec![self.chosen.clone()];
        if !self.alternatives.is_empty() {
            lines.push("Would have come first:".into());
            lines.extend(self.alternatives.iter().map(|line| format!("• {line}")));
        }
        if let Some(ruled_out) = &self.ruled_out {
            lines.push(ruled_out.clone());
        }
        if let Some(plan) = &self.plan {
            lines.push(plan.clone());
        }
        lines.join("\n")
    }
}

/// Present-tense words for a leaf, by action and parameter. The simulation
/// keeps the same table for the running leaf (`resident::label`); a record
/// names passed-over leaves only by action, so they are worded here.
pub(crate) fn action_phrase(action: &str, param: &str) -> String {
    let known = match (action, param) {
        ("life.build", _) => "building",
        ("life.buy", "water") => "buying water",
        ("life.buy", "food") => "buying food",
        ("life.buy", "fuel") => "buying firewood",
        ("life.buy", "provisions" | "reserves") => "stocking up on provisions",
        ("life.buy", "clothes") => "buying warm clothes",
        ("life.buy", "inputs") => "buying work materials",
        ("life.buy", "tool") => "buying a tool",
        ("life.buy", _) => "buying goods",
        ("life.change_job", _) => "taking a new job",
        ("life.collect_found", _) => "picking up lost property",
        ("life.continue", _) => "finishing what they started",
        ("life.craft", _) => "crafting at the workplace",
        ("life.drink", _) => "drinking",
        ("life.eat", _) => "eating",
        ("life.equip", _) => "putting on a coat",
        ("life.gather", "water") => "fetching water",
        ("life.gather", "food") => "foraging",
        ("life.gather", "fuel") => "gathering firewood",
        ("life.gather", _) => "gathering for work",
        ("life.go_to", "home") => "going home",
        ("life.go_to", "work") => "going to work",
        ("life.go_to", "centre") => "strolling to the square",
        ("life.go_to", _) => "taking a walk",
        ("life.haul", "materials") => "carrying building materials",
        ("life.haul", "surplus") => "storing surplus goods",
        ("life.haul", "restock") => "restocking the stall",
        ("life.haul", _) => "carrying firewood home",
        ("life.hunt", _) => "hunting",
        ("life.idle", _) => "standing idle",
        ("life.pursue_goal", _) => "working to a plan",
        ("life.sell", _) => "delivering goods",
        ("life.sleep", _) => "sleeping at home",
        ("life.socialise", _) => "meeting a neighbour",
        ("life.warm_up", _) => "warming up at home",
        ("ExecuteOrder", _) => "following orders",
        _ => "",
    };
    if !known.is_empty() {
        return known.to_owned();
    }
    let action = action
        .rsplit('.')
        .next()
        .unwrap_or(action)
        .replace('_', " ");
    if param.is_empty() {
        action
    } else {
        format!("{action} ({})", param.replace('_', " "))
    }
}

/// A failed start in the player's words. Unknown reasons pass through: they
/// are written as plain sentences already.
fn reason_phrase(reason: &str) -> &str {
    match reason {
        "a coat is already worn or none is carried" => "already wearing one, or has none",
        "a usable tool is already carried" => "already has a working tool",
        "no affordable seller offers hearth fuel" => {
            "nobody sells firewood at a price they can pay"
        }
        "no affordable seller offers it" => "nobody sells it at a price they can pay",
        "no affordable seller offers food" => "nobody sells food at a price they can pay",
        "no affordable seller offers the tool" => "nobody sells the tool at a price they can pay",
        "no building commission" => "nobody has asked them to build",
        "no building commission and no supplied building site" => {
            "nobody has asked them to build, and no site is ready"
        }
        "no supplied building site of this settlement needs a builder" => {
            "no building site in this village is ready for a builder"
        }
        "no building site of this settlement needs materials this resident can bring" => {
            "no building site in this village needs anything they can bring"
        }
        "no stall of this settlement is short of a good its storage holds" => {
            "no stall in this village is short of a good that is in store"
        }
        "no surplus worth a trip, or no store of this settlement has room" => {
            "nothing to spare that is worth the walk, or no store in this village has room"
        }
        "no neighbour has funded demand for it" => "no neighbour can pay for it",
        "no funded demand for a raw good, or no patch has any left" => {
            "no buyer can pay for raw goods, or the patches are empty"
        }
        "no funded production goal has an executable plan" => {
            "no paying order they can fill from where they stand"
        }
        "no funded recipe has its inputs in the pack" => {
            "they lack the ingredients for anything that would sell"
        }
        "no hearth fuel is carried" => "they carry no firewood",
        "no food carried" => "they carry no food",
        "no water carried" => "they carry no water",
        "no reachable patch has any left, or the pack is full" => {
            "no patch in reach has any left, or their pack is full"
        }
        "no requested or better-paid vacancy" => {
            "no open place that you asked for or that pays better"
        }
        "not a hunter with a spear, funded demand and prey in reach" => {
            "it takes a spear, a paying buyer and prey in reach"
        }
        "nothing is in progress" => "nothing was under way",
        "nothing is offered for sale from the pack" => "they have nothing to sell",
        "nothing lost within reach" => "nothing of theirs lies nearby",
        "nothing wanted is on offer at an affordable price" => {
            "nothing they want is for sale at a price they can pay"
        }
        "the route was abandoned after repeated blockage" => "the way there stayed blocked",
        "this resident has no such place" => "they have no such place",
        "it could not start" => "it was not possible just then",
        other => other,
    }
}

/// Reasons that only say an option is not this person's business. They are
/// true, but they explain nothing a player would ask about.
fn beside_the_point(action: &str, reason: &str, hunter: bool) -> bool {
    // Standing chores and errands: when there is none to do, nothing was lost.
    matches!(
        action,
        "life.collect_found" | "life.equip" | "life.change_job" | "life.haul" | "life.continue"
    ) || matches!(
        reason,
        "a usable tool is already carried"
            | "the hearth does not need fuel now"
            | "the hearth does not need fuel"
    ) || (action == "life.build"
        && (reason.starts_with("no building") || reason.starts_with("no supplied")))
        || (action == "life.hunt" && !hunter)
}

/// Why a branch was entered, by the name of the node above the leaf:
/// `(node, now, when it began)`. Trees are world data; an unlisted node is
/// quoted by its own name.
const BRANCH_REASONS: &[(&str, &str, &str)] = &[
    (
        "Critical thirst",
        "they are dangerously thirsty",
        "they were dangerously thirsty",
    ),
    ("Critical hunger", "they are starving", "they were starving"),
    (
        "Player orders",
        "they were given an order",
        "they were given an order",
    ),
    ("Thirst", "they are thirsty", "they were thirsty"),
    ("Hunger", "they are hungry", "they were hungry"),
    ("Tired", "they are tired", "they were tired"),
    ("Cold", "they are cold", "they were cold"),
    ("Bedtime", "it is their bedtime", "it was their bedtime"),
    (
        "Meal time",
        "it is meal time and they carry food",
        "it was meal time",
    ),
    (
        "Hearth needs fuel",
        "the hearth at home is out",
        "the hearth at home was out",
    ),
    (
        "Patch is nearer than a seller",
        "the hearth at home is out and a patch is nearer than any seller",
        "the hearth at home was out",
    ),
    (
        "Has spare coins",
        "they have coins to spare",
        "they had coins to spare",
    ),
    (
        "Chores",
        "it is a chore that was waiting",
        "it was a chore that was waiting",
    ),
    ("Leisure", "it is their free time", "it was their free time"),
    (
        "Work hours",
        "it is their working time",
        "it was their working time",
    ),
    ("Night", "it is night", "it was night"),
    (
        "Seek company",
        "nothing else needs doing",
        "nothing else needed doing",
    ),
    (
        "Idle",
        "nothing needs doing right now",
        "nothing needed doing",
    ),
];

fn branch_reason(branch: &str, began: bool) -> Option<String> {
    let nodes: Vec<&str> = branch.split(" > ").collect();
    // Root first, leaf last: the nearest named node above the leaf explains it.
    let inner = nodes.get(1..nodes.len().saturating_sub(1))?;
    for node in inner.iter().rev() {
        if let Some((_, now, then)) = BRANCH_REASONS.iter().find(|(name, _, _)| name == node) {
            return Some((if began { *then } else { *now }).to_owned());
        }
    }
    // A leaf directly under the root is its own reason.
    if inner.is_empty()
        && let Some((_, now, then)) = BRANCH_REASONS
            .iter()
            .find(|(name, _, _)| nodes.last() == Some(name))
    {
        return Some((if began { *then } else { *now }).to_owned());
    }
    inner
        .first()
        .map(|node| format!("of the rule \u{201c}{node}\u{201d}"))
}

fn activity_of(record: &Value) -> String {
    record["activity"]
        .as_str()
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| {
            action_phrase(
                record["action"].as_str().unwrap_or_default(),
                record["param"].as_str().unwrap_or_default(),
            )
        })
}

struct Blocked {
    action: String,
    param: String,
    reason: String,
}

/// Splits a record's `rejected` text into the conditions that were false and
/// the leaves that could not start. The grammar is the record's own:
/// `<cond> is false`, `<cond> holds, but <rest>`, `<action> <param>: <reason>`,
/// joined by `; `.
fn parse_rejection(text: &str) -> (Vec<String>, Vec<Blocked>) {
    let mut quiet = Vec::new();
    let mut blocked = Vec::new();
    for piece in text.split("; ") {
        let mut rest = piece.trim();
        while let Some((_, tail)) = rest.split_once(" holds, but ") {
            rest = tail;
        }
        if let Some(cond) = rest.strip_suffix(" is false") {
            quiet.push(cond.to_owned());
        } else if let Some((leaf, reason)) = rest.split_once(": ") {
            let (action, param) = leaf.split_once(' ').unwrap_or((leaf, ""));
            blocked.push(Blocked {
                action: action.to_owned(),
                param: param.to_owned(),
                reason: reason.to_owned(),
            });
        } else if !rest.is_empty() {
            blocked.push(Blocked {
                action: String::new(),
                param: String::new(),
                reason: rest.to_owned(),
            });
        }
    }
    (quiet, blocked)
}

const NO_WATER: &str = "no water is carried, for sale at a price they can pay, or in reach";
const NO_FOOD: &str = "no food is carried, for sale at a price they can pay, or in reach";

fn plan_from_record(record: &Value, names: &dyn Fn(&str) -> String) -> Option<PlanView> {
    let plan = record.get("plan")?;
    let goal = plan["goal"].as_str().filter(|goal| !goal.is_empty())?;
    let first = plan["first"].as_str().map(|step| {
        let (verb, what) = step.split_once(' ').unwrap_or((step, ""));
        plan_step(verb, &names(what))
    });
    Some(PlanView {
        goal: names(goal),
        steps: first.into_iter().collect(),
        remaining: plan["steps"].as_u64().unwrap_or(0) as usize,
    })
}

/// One plan step in words; `what` is already a display name.
pub(crate) fn plan_step(verb: &str, what: &str) -> String {
    match verb {
        "craft" => what.to_lowercase(),
        "buy" | "gather" => format!("{verb} {what}"),
        other => format!("{other} {what}").trim().to_owned(),
    }
}

pub(crate) fn plan_line(plan: &PlanView) -> String {
    let count = plan.remaining.max(plan.steps.len());
    let mut line = format!(
        "Plan: supply {}, {count} step{} to go.",
        plan.goal,
        if count == 1 { "" } else { "s" }
    );
    match plan.steps.as_slice() {
        [] => {}
        [next] => line.push_str(&format!(" Next: {next}.")),
        [next, then, ..] if next == then => line.push_str(&format!(" Next: {next}, twice.")),
        [next, then, ..] => line.push_str(&format!(" Next: {next}, then {then}.")),
    }
    line
}

/// Builds the "Why" section from one decision record (the journal's
/// `VillageDecision`, or the live `resident::explain`, which share a shape).
pub(crate) fn why(record: &Value, context: &WhyContext) -> Why {
    let carrying_on = record["action"].as_str() == Some("life.continue");
    // "Carry on" explains nothing by itself: what began the activity does.
    let source = if carrying_on {
        context.origin.unwrap_or(record)
    } else {
        record
    };
    let began = carrying_on && context.origin.is_some();
    let activity = context
        .activity
        .clone()
        .unwrap_or_else(|| activity_of(source));
    let reason = branch_reason(source["branch"].as_str().unwrap_or_default(), began);
    let chosen = if carrying_on && context.origin.is_none() {
        "Finishing what they had started; nothing more urgent has come up.".to_owned()
    } else {
        match (began, reason) {
            (true, Some(reason)) => format!(
                "Still {activity}. It began because {reason}, and nothing more urgent has come up."
            ),
            (true, None) => format!("Still {activity}; nothing more urgent has come up."),
            (false, Some(reason)) => format!("{}, because {reason}.", capitalise(&activity)),
            (false, None) => format!("{}.", capitalise(&activity)),
        }
    };

    let mut quiet: Vec<String> = Vec::new();
    let mut pressing: Vec<String> = Vec::new();
    let mut blocked: Vec<String> = Vec::new();
    let mut weighed: Vec<String> = Vec::new();
    let empty = Vec::new();
    let considered = source["considered"].as_array().unwrap_or(&empty);
    let best = considered
        .iter()
        .filter(|option| option["chosen"].as_bool() == Some(true))
        .find_map(|option| option["score"].as_i64());
    for option in considered {
        let node = option["node"].as_str().unwrap_or("Another option");
        if let Some(score) = option["score"].as_i64() {
            if option["chosen"].as_bool() != Some(true) {
                weighed.push(match best {
                    Some(best) => {
                        format!("{node}: it mattered less just then ({score} against {best}).")
                    }
                    None => format!("{node}: it mattered less just then."),
                });
            }
            continue;
        }
        let Some(rejected) = option["rejected"].as_str() else {
            continue;
        };
        let (false_conds, leaves) = parse_rejection(rejected);
        if leaves.is_empty() {
            quiet.extend(false_conds);
            continue;
        }
        for leaf in leaves {
            if beside_the_point(&leaf.action, &leaf.reason, context.hunter) {
                continue;
            }
            let what = if leaf.action.is_empty() {
                node.to_owned()
            } else {
                capitalise(&action_phrase(&leaf.action, &leaf.param))
            };
            blocked.push(format!("{what}: {}.", reason_phrase(&leaf.reason)));
        }
    }
    // A need that is low although its branch was closed is the first thing a
    // player asks about: the condition also needs a way to satisfy it.
    let has = |cond: &str| quiet.iter().any(|name| name == cond);
    let thirsty = has("life.thirsty") || has("life.critical_thirst");
    let hungry = has("life.hungry") || has("life.critical_hunger");
    if thirsty && context.thirst != NeedLevel::Fine {
        pressing.push(format!("Drinking: thirsty, but {NO_WATER}."));
    }
    if hungry && context.hunger != NeedLevel::Fine {
        pressing.push(format!("Eating: hungry, but {NO_FOOD}."));
    }
    if has("life.cold") && context.warmth != NeedLevel::Fine {
        pressing.push(
            "Warming up at home: cold, but the hearth is out and they carry no firewood, or \
             they already wear a coat."
                .to_owned(),
        );
    }
    let mut ruled_out: Vec<&str> = Vec::new();
    if thirsty && context.thirst == NeedLevel::Fine {
        ruled_out.push(if has("life.thirsty") {
            "not thirsty"
        } else {
            "not dangerously thirsty"
        });
    }
    if hungry && context.hunger == NeedLevel::Fine {
        ruled_out.push(if has("life.hungry") {
            "not hungry"
        } else {
            "not starving"
        });
    }
    for (cond, phrase) in [
        ("life.tired", "not tired"),
        ("life.cold", "not cold"),
        ("life.bedtime", "not bedtime yet"),
        ("life.wants_meal", "no meal due"),
        ("life.wants_leisure", "no free time now"),
        ("life.in_work_window", "outside working hours"),
        ("life.in_sleep_window", "not night"),
    ] {
        if has(cond) && !(cond == "life.cold" && context.warmth != NeedLevel::Fine) {
            ruled_out.push(phrase);
        }
    }

    // The nearest passed-over options are the last ones listed.
    let room = 3_usize.saturating_sub(pressing.len());
    let nearest = blocked.len().saturating_sub(room);
    let mut alternatives = pressing;
    alternatives.extend(blocked.drain(nearest..));
    let room = 3_usize.saturating_sub(alternatives.len());
    alternatives.extend(weighed.into_iter().take(room));

    // A plan the resident still remembers is not why they are asleep: it is
    // named only while the plan is what they are doing.
    let planning = [record, source].iter().any(|entry| {
        entry["action"].as_str() == Some("life.pursue_goal") || entry.get("plan").is_some()
    });
    let plan = planning
        .then(|| {
            context
                .plan
                .clone()
                .or_else(|| plan_from_record(record, context.names))
                .or_else(|| plan_from_record(source, context.names))
        })
        .flatten()
        .map(|plan| plan_line(&plan));
    Why {
        chosen,
        alternatives,
        ruled_out: (!ruled_out.is_empty())
            .then(|| format!("Checked first: {}.", ruled_out.join(", "))),
        plan,
    }
}
