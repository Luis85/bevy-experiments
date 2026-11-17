//! Conditions with parameters and `not` (review fix S3.1), resolved on the
//! island's side: the generic runtime only ever sees plain registered names.
//!
//! A guard in a behaviour document may name a parameterised test:
//!
//! ```json
//! {"Guard": {"if": "need_below", "need": "Hunger", "value": 40000, "not": false, "abort": "None"}}
//! {"Guard": {"if": "life.night", "not": true, "abort": "None"}}
//! {"Guard": {"if": "life.player_within", "value": 6, "abort": "None"}}
//! ```
//!
//! [`rewrite_guards`] turns each into a plain condition name before the tree
//! is parsed (`need_below:Hunger:40000`, `not:life.night`). At registration
//! every distinct such name takes one of [`SLOTS`] condition functions; the
//! [`ParamConds`] table says what each slot tests. A negated guard never
//! aborts lower-priority work (it promises nothing about its branch).
use super::BlockKind;
use crate::village::frontier::economy_spec::{EconomySpec, GoodTag};
use crate::village::{Role, Season, Villager};
use bevy_ecs::prelude::*;
use serde_json::Value;

/// Distinct parameterised conditions one world may use.
pub const SLOTS: usize = 64;

/// What a parameterised condition tests.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ParamCond {
    /// The need is below the value (milli).
    NeedBelow { need: String, value: i32 },
    /// The pack holds at least `count` of the good.
    HasGood { good: String, count: u32 },
    /// The pack holds at least `count` goods with the tag.
    HasTag { tag: GoodTag, count: u32 },
    /// The resident works this trade.
    IsTrade(Role),
    /// It is this season.
    InSeason(Season),
    /// The trait is at least the value (0..=100).
    TraitAtLeast { name: String, value: i32 },
    /// The resident's shifted schedule is in this block.
    InBlock(BlockKind),
    /// The played character is alive and within this many millimetres
    /// (`life.player_within`, authored in metres).
    PlayerWithin { mm: i64 },
    /// A registered condition, negated.
    Not(String),
    /// A parameterised condition, negated.
    NotParam(Box<ParamCond>),
}

/// The slot table, built from the behaviour document at registration.
#[derive(Resource, Clone, Debug, Default)]
pub struct ParamConds {
    pub slots: Vec<(String, ParamCond)>,
}

/// The test of the played character's distance (S3.6, npc-ai#6).
pub const PLAYER_WITHIN: &str = "life.player_within";

const NEEDS: [&str; 4] = ["Hunger", "Thirst", "Energy", "Warmth"];
const TRAITS: [&str; 3] = ["diligence", "sociability", "thrift"];

/// Rewrites every `{"if": ..}` guard of a raw tree into a plain `cond`.
pub fn rewrite_guards(value: &mut Value) -> Result<(), String> {
    match value {
        Value::Object(map) => {
            if let Some(Value::Object(guard)) = map.get_mut("Guard")
                && guard.contains_key("if")
            {
                let name = name_of(guard)?;
                let abort = guard.remove("abort").unwrap_or(Value::from("None"));
                guard.clear();
                guard.insert("cond".into(), Value::from(name));
                guard.insert("abort".into(), abort);
            }
            for child in map.values_mut() {
                rewrite_guards(child)?;
            }
        }
        Value::Array(items) => {
            for item in items {
                rewrite_guards(item)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn name_of(guard: &serde_json::Map<String, Value>) -> Result<String, String> {
    let test = guard
        .get("if")
        .and_then(Value::as_str)
        .ok_or("a guard's `if` must name a test")?;
    let negate = guard.get("not").and_then(Value::as_bool).unwrap_or(false);
    let text = |key: &str| guard.get(key).and_then(Value::as_str).map(str::to_owned);
    let number = |key: &str| guard.get(key).and_then(Value::as_i64);
    let base = match test {
        "need_below" => format!(
            "need_below:{}:{}",
            text("need").ok_or("`need_below` needs `need`")?,
            number("value").ok_or("`need_below` needs `value`")?
        ),
        "has_good" => format!(
            "has_good:{}:{}",
            text("good")
                .or_else(|| text("tag"))
                .ok_or("`has_good` needs `good` or `tag`")?,
            number("value").unwrap_or(1)
        ),
        "is_trade" => format!(
            "is_trade:{}",
            text("trade").ok_or("`is_trade` needs `trade`")?
        ),
        "season" => format!(
            "season:{}",
            text("season").ok_or("`season` needs `season`")?
        ),
        "trait_at_least" => format!(
            "trait_at_least:{}:{}",
            text("trait").ok_or("`trait_at_least` needs `trait`")?,
            number("value").ok_or("`trait_at_least` needs `value`")?
        ),
        "in_block" => format!(
            "in_block:{}",
            text("block").ok_or("`in_block` needs `block`")?
        ),
        PLAYER_WITHIN => format!(
            "{PLAYER_WITHIN}:{}",
            number("value").ok_or("`life.player_within` needs `value` (metres)")?
        ),
        // A registered condition: only `not` makes it a new name.
        plain if negate => return Ok(format!("not:{plain}")),
        plain => return Ok(plain.to_owned()),
    };
    Ok(if negate { format!("not:{base}") } else { base })
}

/// Whether a condition name is one this module resolves.
pub fn is_param(name: &str) -> bool {
    name.starts_with("not:")
        || [
            "need_below:",
            "has_good:",
            "is_trade:",
            "season:",
            "trait_at_least:",
            "in_block:",
            "life.player_within:",
        ]
        .iter()
        .any(|prefix| name.starts_with(prefix))
}

/// Parses a resolved name, checking every part against the world's data.
pub fn parse(name: &str, economy: &EconomySpec) -> Result<ParamCond, String> {
    if let Some(inner) = name.strip_prefix("not:") {
        return if is_param(inner) {
            Ok(ParamCond::NotParam(Box::new(parse(inner, economy)?)))
        } else if super::conditions::find(inner).is_some() {
            Ok(ParamCond::Not(inner.to_owned()))
        } else {
            Err(format!("unknown condition `{inner}` under `not`"))
        };
    }
    let parts: Vec<&str> = name.split(':').collect();
    let int = |text: &str| {
        text.parse::<i64>()
            .map_err(|_| format!("`{text}` is not a number"))
    };
    match parts.as_slice() {
        ["need_below", need, value] => {
            if !NEEDS.contains(need) {
                return Err(format!(
                    "unknown need `{need}`; needs: {}",
                    NEEDS.join(", ")
                ));
            }
            let value = int(value)?;
            if !(0..=100_000).contains(&value) {
                return Err("`need_below` value must be 0..=100000".into());
            }
            Ok(ParamCond::NeedBelow {
                need: (*need).to_owned(),
                value: value as i32,
            })
        }
        ["has_good", good, count] => {
            let count = int(count)?;
            if !(1..=1000).contains(&count) {
                return Err("`has_good` value must be 1..=1000".into());
            }
            if economy.good(good).is_some() {
                return Ok(ParamCond::HasGood {
                    good: (*good).to_owned(),
                    count: count as u32,
                });
            }
            let tag: GoodTag = serde_json::from_value(Value::from(*good))
                .map_err(|_| format!("`{good}` is neither a good nor a tag of the catalog"))?;
            Ok(ParamCond::HasTag {
                tag,
                count: count as u32,
            })
        }
        ["is_trade", trade] => {
            let role = Role::from_key(trade);
            if economy.profession(role).is_none() {
                return Err(format!("`{trade}` is not a trade of the catalog"));
            }
            Ok(ParamCond::IsTrade(role))
        }
        ["season", season] => {
            let season = match *season {
                "spring" | "Spring" => Season::Spring,
                "summer" | "Summer" => Season::Summer,
                "autumn" | "Autumn" => Season::Autumn,
                "winter" | "Winter" => Season::Winter,
                other => return Err(format!("unknown season `{other}`")),
            };
            Ok(ParamCond::InSeason(season))
        }
        ["trait_at_least", name, value] => {
            if !TRAITS.contains(name) {
                return Err(format!(
                    "unknown trait `{name}`; traits: {}",
                    TRAITS.join(", ")
                ));
            }
            let value = int(value)?;
            if !(0..=100).contains(&value) {
                return Err("`trait_at_least` value must be 0..=100".into());
            }
            Ok(ParamCond::TraitAtLeast {
                name: (*name).to_owned(),
                value: value as i32,
            })
        }
        ["in_block", block] => {
            let block = match *block {
                "work" => BlockKind::Work,
                "meal" => BlockKind::Meal,
                "leisure" => BlockKind::Leisure,
                "sleep" => BlockKind::Sleep,
                other => return Err(format!("unknown schedule block `{other}`")),
            };
            Ok(ParamCond::InBlock(block))
        }
        [PLAYER_WITHIN, metres] => {
            let metres = int(metres)?;
            if !(1..=100).contains(&metres) {
                return Err("`life.player_within` value must be 1..=100 (metres)".into());
            }
            Ok(ParamCond::PlayerWithin { mm: metres * 1000 })
        }
        _ => Err(format!("`{name}` is not a condition with parameters")),
    }
}

/// Evaluates one parameterised condition.
fn eval(world: &World, actor: Entity, cond: &ParamCond) -> bool {
    match cond {
        ParamCond::NeedBelow { need, value } => {
            crate::village::economy::need(world, actor, need) < *value
        }
        ParamCond::HasGood { good, count } => {
            crate::village::economy::count(world, actor, good) >= *count
        }
        ParamCond::HasTag { tag, count } => {
            crate::village::catalog::trade_kinds(world)
                .iter()
                .filter(|kind| crate::village::catalog::has_tag(world, kind, *tag))
                .map(|kind| crate::village::economy::count(world, actor, kind))
                .sum::<u32>()
                >= *count
        }
        ParamCond::IsTrade(role) => world
            .get::<Villager>(actor)
            .is_some_and(|person| person.role == *role),
        ParamCond::InSeason(season) => crate::village::calendar(world).1 == *season,
        ParamCond::TraitAtLeast { name, value } => {
            let traits = super::traits(world, actor);
            let have = match name.as_str() {
                "diligence" => traits.diligence,
                "sociability" => traits.sociability,
                _ => traits.thrift,
            };
            have >= *value
        }
        ParamCond::InBlock(block) => super::in_block(world, actor, *block),
        ParamCond::PlayerWithin { mm } => crate::village::live::player_within(world, actor, *mm),
        ParamCond::Not(name) => {
            !super::conditions::find(name).is_some_and(|info| (info.test)(world, actor))
        }
        ParamCond::NotParam(inner) => !eval(world, actor, inner),
    }
}

fn slot<const N: usize>(world: &World, actor: Entity) -> bool {
    world
        .get_resource::<ParamConds>()
        .and_then(|table| table.slots.get(N))
        .is_some_and(|(_, cond)| eval(world, actor, cond))
}

macro_rules! slots {
    ($($n:literal)*) => { [$(slot::<$n> as fn(&World, Entity) -> bool),*] };
}

/// The condition function of each slot.
pub const SLOT_FNS: [fn(&World, Entity) -> bool; SLOTS] = slots!(
    0 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20 21 22 23 24 25 26 27 28 29 30 31
    32 33 34 35 36 37 38 39 40 41 42 43 44 45 46 47 48 49 50 51 52 53 54 55 56 57 58 59 60 61
    62 63
);

/// Every distinct parameterised condition name of the trees, sorted.
pub fn names_in(trees: &[super::TreeFile]) -> Vec<String> {
    let mut names: Vec<String> = trees
        .iter()
        .flat_map(|file| file.tree.nodes.iter())
        .filter_map(|node| match &node.kind {
            npc_framework::ai::NodeKind::Guard { cond, .. } if is_param(&cond.name) => {
                Some(cond.name.clone())
            }
            _ => None,
        })
        .collect();
    names.sort_unstable();
    names.dedup();
    names
}
