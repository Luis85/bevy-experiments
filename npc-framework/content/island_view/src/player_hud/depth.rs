//! Live mode's depth: the goal chain, work progress and repeat orders, the
//! reasons behind low needs, the pack, bonds and gifts, homestead tiers, sleep
//! and the day's account. Everything here reads the simulation's own live
//! accessors (`village::view`) or builds a recorded `LiveAct`; nothing writes
//! simulation state. A slot whose accessor answers nothing is left out.
use super::*;
use island_content::village::{self, live::LiveAct, view};
use npc_framework::kernel::journal::{Journal, JournalKind};
use npc_framework::prelude::{SimCommand, SimIndex, submit};

/// The played character: its identity and its entity.
pub(super) fn played(world: &World) -> Option<(SimId, Entity)> {
    let id = world.get_resource::<village::VillageState>()?.played;
    let entity = world.get_resource::<SimIndex>()?.get(id)?;
    Some((id, entity))
}

/// A recorded live-mode command of the played character, as a view request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LiveRequest {
    pub verb: String,
    pub target: Option<u32>,
    pub kind: String,
    pub quantity: u32,
    /// A recipe repeated at the workshop `target`.
    pub recipe: String,
    /// A garden verb repeated on the plot `target`.
    pub task: String,
}
impl LiveRequest {
    pub(super) fn new(verb: &str) -> Self {
        Self {
            verb: verb.into(),
            target: None,
            kind: String::new(),
            quantity: 1,
            recipe: String::new(),
            task: String::new(),
        }
    }
    pub(super) fn recipe(mut self, recipe: &str) -> Self {
        self.recipe = recipe.into();
        self
    }
    pub(super) fn task(mut self, task: &str) -> Self {
        self.task = task.into();
        self
    }
    pub(super) fn target(mut self, target: u32) -> Self {
        self.target = Some(target);
        self
    }
    pub(super) fn kind(mut self, kind: &str) -> Self {
        self.kind = kind.into();
        self
    }
    pub(super) fn quantity(mut self, quantity: u32) -> Self {
        self.quantity = quantity;
        self
    }
    fn command(&self, unit: SimId) -> LiveAct {
        let mut act = LiveAct::new(unit, &self.verb);
        act.target = self.target.map(SimId);
        act.kind.clone_from(&self.kind);
        act.quantity = self.quantity;
        act.recipe.clone_from(&self.recipe);
        act.task.clone_from(&self.task);
        act
    }
}

/// The simulation's refusal as a sentence: capital first, full stop last.
pub(super) fn sentence(reason: &str) -> String {
    let mut letters = reason.trim().chars();
    let mut out: String = letters.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(letters).collect()
    });
    if !out.is_empty() && !out.ends_with(['.', '!', '?']) {
        out.push('.');
    }
    out
}

/// Whether the simulation would take the request now, and why not.
pub(super) fn check(world: &World, request: &LiveRequest) -> Result<(), String> {
    let (unit, _) = played(world).ok_or("No character is being played.")?;
    request
        .command(unit)
        .check(world)
        .map_err(|reason| sentence(&reason))
}

/// Submit the request as a recorded command; the reason when it is refused.
pub(super) fn send(world: &mut World, request: &LiveRequest) -> Result<(), String> {
    let (unit, _) = played(world).ok_or("No character is being played.")?;
    let act = request.command(unit);
    act.check(world).map_err(|reason| sentence(&reason))?;
    submit(world, None, SimCommand::content(&act))
        .map(|_| ())
        .map_err(|error| sentence(&error.to_string()))
}

/// The goal chain as the tracker shows it.
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct Chain {
    /// 1-based step of the current goal, and the chain's length.
    pub step: u32,
    pub total: u32,
    pub done: u32,
    /// "Unlocks a storage chest", or empty.
    pub reward: String,
    pub current: Option<String>,
    pub next: Option<String>,
    pub last_done: Option<String>,
}

/// The goal chain of the played character; `None` outside live mode.
pub(super) fn chain(world: &World) -> Option<Chain> {
    let (unit, _) = played(world)?;
    let goals = view::goals(world, unit);
    if goals.total == 0 {
        return None;
    }
    let reward = goals
        .current
        .as_ref()
        .map(|goal| reward_words(&goal.reward))
        .unwrap_or_default();
    Some(Chain {
        step: goals.step.min(goals.total),
        total: goals.total,
        done: goals.done.len() as u32,
        reward,
        current: goals.current.map(|goal| goal.title),
        next: goals.next.map(|goal| goal.title),
        last_done: goals.done.last().map(|goal| goal.title.clone()),
    })
}

/// "Unlocks a storage chest and a tool shed"; empty without a reward.
pub(super) fn reward_words(reward: &[String]) -> String {
    match reward {
        [] => String::new(),
        [one] => format!("Unlocks {one}"),
        [rest @ .., last] => format!("Unlocks {} and {last}", rest.join(", ")),
    }
}

/// How far the current work has come, where the simulation knows its
/// length: a rest, a night's sleep or a homestead upgrade.
pub(super) fn work_fraction(world: &World) -> Option<f32> {
    let (_, entity) = played(world)?;
    // The simulation's own measure of the work done, gathering included.
    view::activity_progress_permille(world, entity)
        .map(|permille| permille.min(1000) as f32 / 1000.0)
}

/// The repeat order: what it repeats and "3 of 10".
pub(super) fn repeat_words(world: &World) -> Option<(String, String)> {
    let (unit, _) = played(world)?;
    let (resource, left, done) = view::repeat_order(world, unit)?;
    let what = world
        .get_resource::<SimIndex>()
        .and_then(|index| index.get(resource))
        .and_then(|patch| world.get::<village::ResourcePatch>(patch))
        .map_or_else(
            || "Repeating".to_owned(),
            |patch| format!("Gathering {}", model::friendly(&patch.kind).to_lowercase()),
        );
    Some((what, counted(done, left)))
}

/// "3 of 10": repetitions done of the whole order.
pub(super) fn counted(done: u32, left: u32) -> String {
    format!("{done} of {}", done + left)
}

/// The simulation's name of a need, for an observation key.
pub(super) fn need_name(key: &str) -> Option<&'static str> {
    match key {
        "hunger" => Some("Hunger"),
        "thirst" => Some("Thirst"),
        "energy" => Some("Energy"),
        "warmth" => Some("Warmth"),
        _ => None,
    }
}

/// Why a need of the played character is low, in the simulation's words.
pub(super) fn need_reason(world: &World, key: &str) -> Option<String> {
    let (_, entity) = played(world)?;
    view::need_reason(world, entity, need_name(key)?)
}

/// Whether a good in the pouch helps a need.
pub(super) fn helps(key: &str, kind: &str) -> bool {
    use island_content::village::catalog;
    match key {
        "hunger" => catalog::food_restore(kind).is_some(),
        "thirst" => kind == catalog::WATER,
        "warmth" => kind == catalog::WARM_CLOTHES || kind == "log",
        "health" => catalog::food_restore(kind).is_some() || kind == catalog::WATER,
        _ => false,
    }
}

/// The pack: weight and capacity in kilograms, and the share it fills.
pub(super) fn pack(world: &World) -> Option<(f32, String)> {
    let (_, entity) = played(world)?;
    let pack = view::pack(world, entity)?;
    if pack.max_weight_g == 0 {
        return None;
    }
    Some((
        pack.weight_g as f32 / pack.max_weight_g as f32,
        pack_words(pack.weight_g, pack.max_weight_g),
    ))
}

/// "3.2 of 20 kg".
pub(super) fn pack_words(weight_g: u32, max_g: u32) -> String {
    let kilos = |grams: u32| {
        let tenths = grams.div_ceil(100);
        if tenths.is_multiple_of(10) {
            format!("{}", tenths / 10)
        } else {
            format!("{}.{}", tenths / 10, tenths % 10)
        }
    };
    format!("{} of {} kg", kilos(weight_g), kilos(max_g))
}

/// The tool in hand and its uses left.
pub(super) fn held_tool(world: &World) -> Option<(String, u32)> {
    let (_, entity) = played(world)?;
    view::held_tool(world, entity)
}

/// A friendship tier as the data defines it.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Tier {
    pub name: String,
    pub score: i32,
    pub discount: u32,
    pub helps: bool,
    pub invites: bool,
}

/// The friendship tiers of the world's data, ascending.
pub(super) fn tiers(world: &World) -> Vec<Tier> {
    village::frontier::economy_spec::registry(world)
        .map(|registry| {
            registry
                .spec
                .live
                .tiers
                .iter()
                .map(|tier| Tier {
                    name: tier.name.clone(),
                    score: tier.score,
                    discount: tier.discount_percent,
                    helps: tier.helps,
                    invites: tier.invites,
                })
                .collect()
        })
        .unwrap_or_default()
}

/// "Acquaintance", "Close friend": a tier's name as a word.
pub(super) fn tier_word(name: &str) -> String {
    let mut letters = name.chars();
    letters.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(letters).collect()
    })
}

/// What a tier gives: "10% off what you buy from them · helps you".
pub(super) fn tier_gives(tier: &Tier) -> String {
    let mut parts = Vec::new();
    if tier.discount > 0 {
        parts.push(format!("{}% off what you buy from them", tier.discount));
    }
    if tier.helps {
        parts.push("lends a hand at your homestead".to_owned());
    }
    if tier.invites {
        parts.push("invites you to gatherings".to_owned());
    }
    if parts.is_empty() {
        "Nothing yet. Talk, buy and sell, and give gifts to grow closer.".to_owned()
    } else {
        sentence(&parts.join(" · "))
    }
}

/// The bond of the played character with one resident: tier index and name.
pub(super) fn bond(world: &World, resident: u32) -> (u32, String) {
    played(world)
        .and_then(|(unit, _)| {
            view::relationships(world, unit)
                .into_iter()
                .find(|relation| relation.other.0 == resident && relation.with_player)
        })
        .map_or_else(
            || {
                (
                    0,
                    tiers(world)
                        .first()
                        .map_or_else(|| "stranger".to_owned(), |tier| tier.name.clone()),
                )
            },
            |relation| (relation.tier_index, relation.tier),
        )
}

/// A resident's liked and disliked gift, once the player can know it: the
/// simulation's bond says so (`tastes_known`, after a first gift or at a
/// tier that lends a hand).
pub(super) fn known_tastes(world: &World, resident: u32) -> Option<(String, String)> {
    let (unit, _) = played(world)?;
    view::relationships(world, unit)
        .into_iter()
        .find(|relation| relation.other.0 == resident && relation.with_player)
        .filter(|relation| relation.tastes_known)
        .and_then(|relation| relation.tastes)
}

/// A resident's name, or "A neighbour".
pub(super) fn name_of(world: &World, id: SimId) -> String {
    world
        .get_resource::<SimIndex>()
        .and_then(|index| index.get(id))
        .and_then(|entity| world.get::<village::Villager>(entity))
        .map_or_else(|| "A neighbour".to_owned(), |person| person.name.clone())
}

/// The sentence of a live-mode outcome for the played character, or
/// `None` when the effect is not one of them or not about them.
pub(super) fn outcome(
    kind: &str,
    read: &dyn Fn(&str) -> Option<Value>,
    subject: Option<SimId>,
    player: SimId,
    name: &dyn Fn(SimId) -> String,
) -> Option<String> {
    let read_id = |key: &str| {
        read(key)
            .and_then(|value| value.as_u64())
            .and_then(|value| u32::try_from(value).ok())
            .map(SimId)
    };
    let text = |key: &str| {
        read(key)
            .and_then(|value| value.as_str().map(str::to_owned))
            .unwrap_or_default()
    };
    let good = |key: &str| model::friendly(&text(key)).to_lowercase();
    let mine = subject == Some(player);
    let to_me = read_id("to") == Some(player);
    Some(match kind {
        "VillageGift" if mine => {
            let who = read_id("to").map_or_else(|| "Your neighbour".to_owned(), name);
            match text("taste").as_str() {
                "liked" => format!("{who} loved the {}.", good("kind")),
                "disliked" => format!("{who} did not care for the {}.", good("kind")),
                _ => format!("{who} thanked you for the {}.", good("kind")),
            }
        }
        "VillageGiftReturned" if to_me => {
            let who = subject.map_or_else(|| "A friend".to_owned(), name);
            format!("{who} gave you some {}.", good("kind"))
        }
        "VillageBondTier" if mine => {
            let who = read_id("with").map_or_else(|| "A neighbour".to_owned(), name);
            let tier = text("tier");
            if read("up").and_then(|value| value.as_bool()) == Some(false) {
                format!("You and {who} have drifted apart: now {tier}.")
            } else {
                format!("{who} is now your {tier}.")
            }
        }
        "VillageGreeting" if to_me => {
            let who = subject.map_or_else(|| "A neighbour".to_owned(), name);
            let said = text("text");
            if said.is_empty() {
                format!("{who} greets you.")
            } else {
                format!("{who}: \u{201c}{said}\u{201d}")
            }
        }
        "VillageInvitation" if to_me => {
            let who = subject.map_or_else(|| "A friend".to_owned(), name);
            format!("{who} invites you to the next gathering.")
        }
        "VillageFriendHelped" => {
            let who = subject.map_or_else(|| "A friend".to_owned(), name);
            format!("{who} lent a hand at your homestead.")
        }
        "VillageRepeatStopped" if mine => {
            let detail = text("detail");
            if text("reason") == "count_done" {
                "Your repeat order is done.".to_owned()
            } else if detail.is_empty() {
                "Your repeat order stopped.".to_owned()
            } else {
                format!(
                    "Your repeat order stopped: {}.",
                    detail.trim_end_matches('.')
                )
            }
        }
        "VillageHomesteadOrder" if mine => {
            let quantity = read("quantity")
                .and_then(|value| value.as_u64())
                .unwrap_or(0);
            let coins = read("escrow").and_then(|value| value.as_u64()).unwrap_or(0);
            format!(
                "Ordered {quantity} {} for {coins} coins; it comes to your door.",
                good("kind")
            )
        }
        "VillageHomesteadBuilderHired" if mine => {
            let who = read_id("builder").map_or_else(|| "A builder".to_owned(), name);
            format!("{who} is hired to build for you.")
        }
        "VillageHomesteadUpgradeStarted" if mine => {
            format!("Work on {} has begun.", text("name"))
        }
        "VillageHomesteadUpgraded" if mine => sentence(&format!("{} is finished", text("name"))),
        "VillageGatheringBegan" => format!("{} has begun in the square.", text("name")),
        _ => return None,
    })
}

/// What the live outcome harvest last read in the journal.
#[derive(Resource, Default)]
pub(super) struct OutcomeCursor(Option<(u64, u32)>);

/// New live-mode outcomes of the played character since the last call,
/// oldest first. The journal is a bounded ring: one pass over its tail.
pub(super) fn outcomes(world: &mut World) -> Vec<String> {
    world.init_resource::<OutcomeCursor>();
    let Some((player, _)) = played(world) else {
        return Vec::new();
    };
    let cursor = world.resource::<OutcomeCursor>().0;
    let Some(journal) = world.get_resource::<Journal>() else {
        return Vec::new();
    };
    let latest = journal.entries.back().map(|entry| (entry.tick, entry.seq));
    if latest == cursor {
        return Vec::new();
    }
    // The first look only marks the place: old outcomes are not news.
    let fresh: Vec<_> = if cursor.is_none() || latest < cursor {
        Vec::new()
    } else {
        journal
            .entries
            .iter()
            .rev()
            .take_while(|entry| cursor.is_none_or(|cursor| (entry.tick, entry.seq) > cursor))
            .filter_map(|entry| match &entry.kind {
                JournalKind::Effect {
                    effect,
                    applied: true,
                } => Some((effect.clone(), entry.unit)),
                _ => None,
            })
            .collect()
    };
    world.resource_mut::<OutcomeCursor>().0 = latest;
    let name = |id: SimId| name_of(world, id);
    let mut out: Vec<String> = fresh
        .iter()
        .rev()
        .filter_map(|(effect, unit)| {
            let kind = effect.get("type").and_then(|value| value.as_str())?;
            let read = |key: &str| effect.get(key).cloned();
            outcome(kind, &read, *unit, player, &name)
        })
        .collect();
    out.dedup();
    out
}

/// What the journal says of the played character's day beyond the
/// simulation's account: bonds that changed tier and homestead work.
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct DayEvents {
    pub friends: Vec<String>,
    pub homestead: Vec<String>,
}

/// The day's bond changes and homestead steps, from the journal: the
/// entries of the played character on `day` (the simulation's day count).
pub(super) fn day_events(world: &World, day: u64) -> DayEvents {
    let mut events = DayEvents::default();
    let Some((player, _)) = played(world) else {
        return events;
    };
    let Some(journal) = world.get_resource::<Journal>() else {
        return events;
    };
    let length = world
        .get_resource::<npc_framework::prelude::SimConfig>()
        .map_or(2_400, |config| config.day_length.max(1));
    // The day's ticks by the simulation's own calendar: day 0 of the
    // count is tick 0's day.
    let Some(first) = day
        .checked_sub(village::calendar_on(world, 0).0)
        .map(|days| days * length)
    else {
        return events;
    };
    for entry in journal.entries.iter() {
        if entry.tick < first || entry.tick >= first + length || entry.unit != Some(player) {
            continue;
        }
        let JournalKind::Effect {
            effect,
            applied: true,
        } = &entry.kind
        else {
            continue;
        };
        let text = |key: &str| {
            effect
                .get(key)
                .and_then(|value| value.as_str())
                .unwrap_or_default()
                .to_owned()
        };
        match effect.get("type").and_then(|value| value.as_str()) {
            Some("VillageBondTier") => {
                let who = effect
                    .get("with")
                    .and_then(|value| value.as_u64())
                    .map_or_else(
                        || "A neighbour".to_owned(),
                        |id| name_of(world, SimId(id as u32)),
                    );
                let up = effect.get("up").and_then(|value| value.as_bool()) != Some(false);
                events.friends.push(if up {
                    format!("{who} became your {}.", text("tier"))
                } else {
                    format!("You and {who} drifted apart.")
                });
            }
            Some("VillageHomesteadUpgradeStarted") => {
                events.homestead.push(format!("Began {}.", text("name")));
            }
            Some("VillageHomesteadUpgraded") => {
                events
                    .homestead
                    .push(sentence(&format!("Finished {}", text("name"))));
            }
            Some("VillageCottageBuilt") => events.homestead.push("Built your cottage.".into()),
            _ => {}
        }
    }
    events.friends.dedup();
    events.homestead.dedup();
    events
}

/// One line of the day's account: a label and what it says.
pub(super) type SummaryLine = (&'static str, String);

/// The day summary as lines, one screenful: coins, goods, people, the low
/// point of each need, and the next step of the goal chain.
pub(super) fn summary_lines(
    summary: &village::live::DaySummary,
    name: &dyn Fn(SimId) -> String,
    next: Option<&str>,
    events: &DayEvents,
) -> Vec<SummaryLine> {
    let goods = |list: &std::collections::BTreeMap<String, u32>| {
        list.iter()
            .map(|(kind, count)| crate::game_ui::words::count(u64::from(*count), kind))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let mut lines = Vec::new();
    if summary.earned > 0 || summary.spent > 0 {
        lines.push((
            "Coins",
            format!("Earned {}, spent {}", summary.earned, summary.spent),
        ));
    }
    // What trade moved this day, with what it cost or brought in.
    for (label, list, coins) in [
        ("Bought", &summary.bought, summary.bought_coins),
        ("Sold", &summary.sold, summary.sold_coins),
    ] {
        if !list.is_empty() {
            let text = if coins > 0 {
                format!("{} for {coins} coins", goods(list))
            } else {
                goods(list)
            };
            lines.push((label, text));
        }
    }
    // A purchase is also booked as received; Bought says it once.
    let received: std::collections::BTreeMap<String, u32> = summary
        .received
        .iter()
        .map(|(kind, count)| {
            let bought = summary.bought.get(kind).copied().unwrap_or(0);
            (kind.clone(), count.saturating_sub(bought))
        })
        .filter(|(_, count)| *count > 0)
        .collect();
    for (label, list) in [
        ("Gathered", &summary.gathered),
        ("Made", &summary.made),
        ("Received", &received),
    ] {
        if !list.is_empty() {
            lines.push((label, goods(list)));
        }
    }
    if !summary.met.is_empty() {
        lines.push((
            "People met",
            summary
                .met
                .iter()
                .map(|id| name(*id))
                .collect::<Vec<_>>()
                .join(", "),
        ));
    }
    if !events.friends.is_empty() {
        lines.push(("Friends", events.friends.join(" ")));
    }
    if !events.homestead.is_empty() {
        lines.push(("Homestead", events.homestead.join(" ")));
    }
    if !summary.low.is_empty() {
        let words = [
            ("Hunger", "Food"),
            ("Thirst", "Water"),
            ("Energy", "Energy"),
            ("Warmth", "Warmth"),
        ];
        let low = words
            .iter()
            .filter_map(|(need, word)| {
                summary
                    .low
                    .get(*need)
                    .map(|value| format!("{word} {}%", (*value).max(0) / 1000))
            })
            .collect::<Vec<_>>()
            .join(" · ");
        lines.push(("Lowest", low));
    }
    if let Some(next) = next {
        lines.push(("Next", next.to_owned()));
    }
    lines
}

/// The goods a gift can be: what the pouch holds that the data lets one give.
pub(super) fn giftable(world: &World, stock: &BTreeMap<String, u64>) -> Vec<(String, u64)> {
    let allowed: Vec<String> = village::frontier::economy_spec::registry(world)
        .map(|registry| registry.spec.live.gift_goods.clone())
        .unwrap_or_default();
    stock
        .iter()
        .filter(|(kind, count)| **count > 0 && allowed.iter().any(|good| good == *kind))
        .map(|(kind, count)| (kind.clone(), *count))
        .collect()
}
