//! Oversee controls of the living world (roadmap phase 4): profession
//! targets and village priorities.
//!
//! Both are standing wishes of the overseer, set by recorded commands and
//! read by the existing rules; neither orders anybody about.
//!
//! - A **profession target** is a wanted headcount of one trade in one
//!   settlement. The labour review ([`super::labor`]) treats a trade under
//!   its target like an undersupplied one and a trade over it like a surplus,
//!   through the usual trade-change rules: cooldowns, review intervals and
//!   the safe boundary (nobody leaves mid-task). Cleared, the village decides
//!   again.
//! - A **village priority** is a level (low, normal, high) over one thing the
//!   civic planner or the production plan already chooses: housing, storage,
//!   workplaces (all, or of one trade), the food reserve, the fuel reserve,
//!   coats and the boat's orders. Normal everywhere is the behaviour without
//!   priorities, exactly.
use super::{economy_spec, settlements};
use crate::village::{self, Role, Villager};
use bevy_ecs::prelude::*;
use npc_framework::kernel::commands::CommandError;
use npc_framework::kernel::hash::SimHashResource;
use npc_framework::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::json;

/// `a word` or `an word`, by the word's first letter.
pub fn with_article(word: &str) -> String {
    let vowel = word
        .chars()
        .next()
        .is_some_and(|c| matches!(c.to_ascii_lowercase(), 'a' | 'e' | 'i' | 'o' | 'u'));
    format!("{} {word}", if vowel { "an" } else { "a" })
}

/// The largest wanted headcount of one trade in one settlement.
pub const MAX_TARGET: u32 = 40;

fn unavailable(reason: String) -> CommandError {
    CommandError::Unavailable { reason }
}

/// One wanted headcount.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TargetEntry {
    pub settlement: SimId,
    pub profession: Role,
    pub wanted: u32,
}

/// One priority level other than normal.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PriorityEntry {
    pub settlement: SimId,
    pub topic: PriorityTopic,
    /// The trade of a `workplace` priority; none for every other topic and
    /// for the priority of every workplace.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profession: Option<Role>,
    pub level: PriorityLevel,
}

/// The overseer's standing wishes, sorted by settlement and key.
#[derive(Resource, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Oversight {
    pub targets: Vec<TargetEntry>,
    pub priorities: Vec<PriorityEntry>,
}
impl SimHashResource for Oversight {
    const NAME: &'static str = "living_oversight_v1";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.u32(self.targets.len() as u32);
        for entry in &self.targets {
            h.u32(entry.settlement.0);
            entry.profession.sim_hash(h);
            h.u32(entry.wanted);
        }
        h.u32(self.priorities.len() as u32);
        for entry in &self.priorities {
            h.u32(entry.settlement.0);
            h.u8(entry.topic as u8);
            h.opt(entry.profession.as_ref(), |h, role| role.sim_hash(h));
            h.u8(entry.level as u8);
        }
    }
    fn snapshot_fns() -> Option<npc_framework::snapshot::ResourceSnapshotFns> {
        Some(npc_framework::snapshot::serde_resource_fns::<Self>())
    }
}

pub fn register(app: &mut bevy_app::App) {
    app.init_resource::<Oversight>()
        .register_sim_resource::<Oversight>()
        .register_command::<SetProfessionTarget>()
        .register_command::<SetVillagePriority>();
}

// ---------------------------------------------------------------------------
// Profession targets
// ---------------------------------------------------------------------------

/// The wanted headcount of `role` in `settlement`; `None`: the village decides.
pub fn target(world: &World, settlement: SimId, role: Role) -> Option<u32> {
    world.get_resource::<Oversight>().and_then(|oversight| {
        oversight
            .targets
            .iter()
            .find(|entry| entry.settlement == settlement && entry.profession == role)
            .map(|entry| entry.wanted)
    })
}

/// Living residents of `settlement` working `role`.
pub fn headcount(world: &World, settlement: SimId, role: Role) -> u32 {
    village::demography::members(world, settlement)
        .into_iter()
        .filter(|(_, actor)| {
            village::living(world, *actor)
                && world
                    .get::<Villager>(*actor)
                    .is_some_and(|person| person.role == role)
        })
        .count() as u32
}

/// How far `role` in `settlement` is below its target (0 at or above it, or
/// without one).
pub fn below_target(world: &World, settlement: SimId, role: Role) -> u32 {
    target(world, settlement, role).map_or(0, |wanted| {
        wanted.saturating_sub(headcount(world, settlement, role))
    })
}

/// Whether `role` in `settlement` has a target it already meets or exceeds.
pub fn at_or_over_target(world: &World, settlement: SimId, role: Role) -> bool {
    target(world, settlement, role)
        .is_some_and(|wanted| headcount(world, settlement, role) >= wanted)
}

/// Whether `role` in `settlement` is above its target.
pub fn over_target(world: &World, settlement: SimId, role: Role) -> bool {
    target(world, settlement, role)
        .is_some_and(|wanted| headcount(world, settlement, role) > wanted)
}

/// Sets, raises, lowers or clears (`wanted: null`) the wanted headcount of
/// one trade in one settlement.
///
/// `{"type": "SetProfessionTarget", "settlement": 3, "profession": "potter",
///   "wanted": 2}`
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetProfessionTarget {
    pub settlement: SimId,
    pub profession: Role,
    #[serde(default)]
    pub wanted: Option<u32>,
}
impl SetProfessionTarget {
    pub fn check(&self, world: &World) -> Result<(), String> {
        if settlements::definition(world, self.settlement).is_none() {
            return Err(format!("{} is not a settlement", self.settlement.0));
        }
        let registry = economy_spec::registry(world).ok_or("the world has no catalog")?;
        if registry.definition(self.profession).is_none() {
            return Err(format!(
                "`{}` is not a trade of this world's catalog",
                self.profession.key()
            ));
        }
        if economy_spec::is_service(world, self.profession) {
            return Err(format!(
                "{} keeps each of its buildings alone: build or remove one instead",
                with_article(economy_spec::trade_label(world, self.profession))
            ));
        }
        if let Some(wanted) = self.wanted
            && wanted > MAX_TARGET
        {
            return Err(format!("a target is at most {MAX_TARGET} workers"));
        }
        Ok(())
    }
    pub fn perform(&self, world: &mut World) -> Result<(), String> {
        self.check(world)?;
        let previous = target(world, self.settlement, self.profession);
        {
            let mut oversight = world.resource_mut::<Oversight>();
            oversight.targets.retain(|entry| {
                !(entry.settlement == self.settlement && entry.profession == self.profession)
            });
            if let Some(wanted) = self.wanted {
                oversight.targets.push(TargetEntry {
                    settlement: self.settlement,
                    profession: self.profession,
                    wanted,
                });
                oversight.targets.sort_by(|a, b| {
                    (a.settlement, a.profession).cmp(&(b.settlement, b.profession))
                });
            }
        }
        // Moves happen at the next labour reviews, never at once.
        village::production::invalidate(world);
        record_effect(
            world,
            None,
            "LivingProfessionTargetSet",
            &json!({"settlement":self.settlement,"profession":self.profession.key(),
                "trade":economy_spec::trade_label(world, self.profession),
                "wanted":self.wanted,"previous":previous,
                "current":headcount(world, self.settlement, self.profession)}),
        );
        Ok(())
    }
}
impl ContentCommand for SetProfessionTarget {
    const NAME: &'static str = "SetProfessionTarget";
    fn validate(&self, world: &World) -> Result<(), CommandError> {
        self.check(world).map_err(unavailable)
    }
    fn apply(self, world: &mut World) {
        let _ = self.perform(world);
    }
}

/// One row of the Oversee profession table.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ProfessionRowView {
    /// Data key of the trade (`"water_keeper"`, `"potter"`): what the
    /// command's `profession` takes.
    pub key: String,
    /// The trade as players read it.
    pub display: String,
    /// Living residents of the settlement working it now.
    pub current: u32,
    /// The overseer's target; `None`: the village decides.
    pub wanted: Option<u32>,
    /// Free seats at its workplaces in the settlement.
    pub free_seats: u32,
    /// Whether a target may be set (a service trade such as the inn may not).
    pub adjustable: bool,
    /// Why `current` differs from `wanted`, in one sentence; `None` when they
    /// match or there is no target.
    pub reason: Option<String>,
}

/// The Oversee profession table of a settlement: every trade of the catalog,
/// in role order.
pub fn profession_table(world: &World, settlement: SimId) -> Vec<ProfessionRowView> {
    economy_spec::trades(world)
        .into_iter()
        .map(|role| {
            let current = headcount(world, settlement, role);
            let wanted = target(world, settlement, role);
            let (seats, taken) = super::labor::seats_in(world, settlement, role);
            ProfessionRowView {
                key: role.key().to_owned(),
                display: economy_spec::trade_label(world, role).to_owned(),
                current,
                wanted,
                free_seats: seats.saturating_sub(taken),
                adjustable: !economy_spec::is_service(world, role),
                reason: wanted.filter(|wanted| *wanted != current).map(|wanted| {
                    gap_reason(world, settlement, role, current, wanted, seats, taken)
                }),
            }
        })
        .collect()
}

fn gap_reason(
    world: &World,
    settlement: SimId,
    role: Role,
    current: u32,
    wanted: u32,
    seats: u32,
    taken: u32,
) -> String {
    let trade = economy_spec::trade_label(world, role);
    if current > wanted {
        if super::labor::essential_kept(world, settlement, role) && wanted == 0 {
            return format!("The last {trade} stays while the village still wants its goods.");
        }
        return format!(
            "{} {trade}(s) too many: they move to other work at their next review, when a \
             trade they may take has a free seat.",
            current - wanted
        );
    }
    if seats == 0 {
        let building = workplace_building(world, role);
        return format!(
            "No {trade} workplace stands here: build {}.",
            with_article(&building)
        );
    }
    if taken >= seats {
        let building = workplace_building(world, role);
        return format!("Every {trade} seat is taken: build another {building}.");
    }
    if !super::labor::anyone_may_take(world, settlement, role) {
        return format!("Nobody here has the aptitude for the {trade}'s trade.");
    }
    format!(
        "{} more {trade}(s) wanted: residents move at their next review, once their \
         switching rest is over and they are between tasks.",
        wanted - current
    )
}

fn workplace_building(world: &World, role: Role) -> String {
    economy_spec::registry(world)
        .and_then(|registry| {
            registry.spec.buildings.iter().find(|building| {
                matches!(&building.kind, super::building_spec::BuildingKind::Workplace { profession, .. } if *profession == role)
            })
        })
        .map_or_else(|| "workplace".to_owned(), |building| building.display.to_lowercase())
}

// ---------------------------------------------------------------------------
// Village priorities
// ---------------------------------------------------------------------------

/// What a priority weighs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PriorityTopic {
    Housing,
    Storage,
    /// Workplaces: of one trade with `profession`, of all without.
    Workplace,
    FoodReserve,
    FuelReserve,
    Coats,
    BoatOrders,
}
impl PriorityTopic {
    pub const ALL: [Self; 7] = [
        Self::Housing,
        Self::Storage,
        Self::Workplace,
        Self::FoodReserve,
        Self::FuelReserve,
        Self::Coats,
        Self::BoatOrders,
    ];
    pub fn key(self) -> &'static str {
        match self {
            Self::Housing => "housing",
            Self::Storage => "storage",
            Self::Workplace => "workplace",
            Self::FoodReserve => "food_reserve",
            Self::FuelReserve => "fuel_reserve",
            Self::Coats => "coats",
            Self::BoatOrders => "boat_orders",
        }
    }
    pub fn display(self) -> &'static str {
        match self {
            Self::Housing => "Housing",
            Self::Storage => "Storage",
            Self::Workplace => "Workplaces",
            Self::FoodReserve => "Food reserve",
            Self::FuelReserve => "Fuel reserve",
            Self::Coats => "Coats",
            Self::BoatOrders => "Boat orders",
        }
    }
}

/// How much a topic weighs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PriorityLevel {
    Low,
    #[default]
    Normal,
    High,
}

/// The level of `topic` (for a workplace, of `profession`'s, falling back to
/// the level of every workplace) in `settlement`.
pub fn level(
    world: &World,
    settlement: SimId,
    topic: PriorityTopic,
    profession: Option<Role>,
) -> PriorityLevel {
    let Some(oversight) = world.get_resource::<Oversight>() else {
        return PriorityLevel::Normal;
    };
    let find = |profession: Option<Role>| {
        oversight
            .priorities
            .iter()
            .find(|entry| {
                entry.settlement == settlement
                    && entry.topic == topic
                    && entry.profession == profession
            })
            .map(|entry| entry.level)
    };
    profession
        .and_then(|role| find(Some(role)))
        .or_else(|| find(None))
        .unwrap_or_default()
}

/// Reviews a civic need must last before the planner builds for it: half at
/// high (at least one), twice at low, unchanged at normal.
pub fn sustain(level: PriorityLevel, normal: u32) -> u32 {
    match level {
        PriorityLevel::High => (normal / 2).max(1),
        PriorityLevel::Normal => normal,
        PriorityLevel::Low => normal.saturating_mul(2).max(1),
    }
}

/// A reserve scaled by its level: doubled at high, halved at low (at least
/// one when it was any), unchanged at normal.
pub fn scale(level: PriorityLevel, normal: u32) -> u32 {
    match level {
        PriorityLevel::High => normal.saturating_mul(2),
        PriorityLevel::Normal => normal,
        PriorityLevel::Low if normal == 0 => 0,
        PriorityLevel::Low => (normal / 2).max(1),
    }
}

/// Orders due civic projects: higher level first, otherwise the planner's
/// own order (the input order) is kept. All normal: unchanged.
pub fn order_projects<T>(due: Vec<(PriorityLevel, T)>) -> Vec<T> {
    let mut due: Vec<(usize, PriorityLevel, T)> = due
        .into_iter()
        .enumerate()
        .map(|(index, (level, item))| (index, level, item))
        .collect();
    due.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    due.into_iter().map(|(_, _, item)| item).collect()
}

/// Sets one priority of a settlement; `normal` removes it.
///
/// `{"type": "SetVillagePriority", "settlement": 3, "topic": "housing",
///   "level": "high"}`; a trade's workplaces:
/// `{"type": "SetVillagePriority", "settlement": 3, "topic": "workplace",
///   "profession": "potter", "level": "high"}`
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetVillagePriority {
    pub settlement: SimId,
    pub topic: PriorityTopic,
    #[serde(default)]
    pub profession: Option<Role>,
    pub level: PriorityLevel,
}
impl SetVillagePriority {
    pub fn check(&self, world: &World) -> Result<(), String> {
        if settlements::definition(world, self.settlement).is_none() {
            return Err(format!("{} is not a settlement", self.settlement.0));
        }
        match (self.topic, self.profession) {
            (PriorityTopic::Workplace, Some(role)) => {
                let known = economy_spec::registry(world)
                    .is_some_and(|registry| registry.definition(role).is_some());
                if !known {
                    return Err(format!(
                        "`{}` is not a trade of this world's catalog",
                        role.key()
                    ));
                }
            }
            (PriorityTopic::Workplace, None) => {}
            (topic, Some(_)) => {
                return Err(format!(
                    "only a workplace priority names a trade, not `{}`",
                    topic.key()
                ));
            }
            (_, None) => {}
        }
        Ok(())
    }
    pub fn perform(&self, world: &mut World) -> Result<(), String> {
        self.check(world)?;
        let previous = level(world, self.settlement, self.topic, self.profession);
        {
            let mut oversight = world.resource_mut::<Oversight>();
            oversight.priorities.retain(|entry| {
                !(entry.settlement == self.settlement
                    && entry.topic == self.topic
                    && entry.profession == self.profession)
            });
            if self.level != PriorityLevel::Normal {
                oversight.priorities.push(PriorityEntry {
                    settlement: self.settlement,
                    topic: self.topic,
                    profession: self.profession,
                    level: self.level,
                });
                oversight.priorities.sort_by(|a, b| {
                    (a.settlement, a.topic, a.profession).cmp(&(
                        b.settlement,
                        b.topic,
                        b.profession,
                    ))
                });
            }
        }
        village::production::invalidate(world);
        record_effect(
            world,
            None,
            "LivingVillagePrioritySet",
            &json!({"settlement":self.settlement,"topic":self.topic,
                "profession":self.profession.map(Role::key),"level":self.level,
                "previous":previous}),
        );
        Ok(())
    }
}
impl ContentCommand for SetVillagePriority {
    const NAME: &'static str = "SetVillagePriority";
    fn validate(&self, world: &World) -> Result<(), CommandError> {
        self.check(world).map_err(unavailable)
    }
    fn apply(self, world: &mut World) {
        let _ = self.perform(world);
    }
}

/// One row of the Oversee priorities panel.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PriorityRowView {
    /// `housing`, `storage`, `workplace`, `food_reserve`, `fuel_reserve`,
    /// `coats` or `boat_orders`: what the command's `topic` takes.
    pub topic: &'static str,
    pub display: String,
    /// The trade of a per-trade workplace row (its data key).
    pub profession: Option<String>,
    /// `low`, `normal` or `high`.
    pub level: PriorityLevel,
    /// What the current level changes, in plain words.
    pub effect: String,
}

/// The priorities of a settlement: the seven topics, then one row per trade
/// whose workplaces have their own level.
pub fn priorities(world: &World, settlement: SimId) -> Vec<PriorityRowView> {
    let mut rows: Vec<PriorityRowView> = PriorityTopic::ALL
        .into_iter()
        .map(|topic| {
            let level = level(world, settlement, topic, None);
            PriorityRowView {
                topic: topic.key(),
                display: topic.display().to_owned(),
                profession: None,
                level,
                effect: effect(world, topic, None, level),
            }
        })
        .collect();
    if let Some(oversight) = world.get_resource::<Oversight>() {
        for entry in &oversight.priorities {
            if entry.settlement != settlement {
                continue;
            }
            let Some(role) = entry.profession else {
                continue;
            };
            let trade = economy_spec::trade_label(world, role);
            rows.push(PriorityRowView {
                topic: entry.topic.key(),
                display: format!("Workplaces: {trade}"),
                profession: Some(role.key().to_owned()),
                level: entry.level,
                effect: effect(world, entry.topic, Some(role), entry.level),
            });
        }
    }
    rows
}

/// The plain sentence of what `level` does to `topic`.
pub fn effect(
    world: &World,
    topic: PriorityTopic,
    profession: Option<Role>,
    level: PriorityLevel,
) -> String {
    let what = match (topic, profession) {
        (PriorityTopic::Workplace, Some(role)) => {
            format!("{} workplaces", economy_spec::trade_label(world, role))
        }
        (PriorityTopic::Workplace, None) => "workplaces".to_owned(),
        (PriorityTopic::Housing, _) => "homes".to_owned(),
        (PriorityTopic::Storage, _) => "stores".to_owned(),
        (PriorityTopic::FoodReserve, _) => "food".to_owned(),
        (PriorityTopic::FuelReserve, _) => "hearth fuel".to_owned(),
        (PriorityTopic::Coats, _) => "coats".to_owned(),
        (PriorityTopic::BoatOrders, _) => "the boat's orders".to_owned(),
    };
    match (topic, level) {
        (_, PriorityLevel::Normal) => "Normal: the village decides as usual.".to_owned(),
        (
            PriorityTopic::Housing | PriorityTopic::Storage | PriorityTopic::Workplace,
            PriorityLevel::High,
        ) => format!(
            "High: the planner builds {what} after half the usual waiting and before other \
             projects."
        ),
        (
            PriorityTopic::Housing | PriorityTopic::Storage | PriorityTopic::Workplace,
            PriorityLevel::Low,
        ) => format!(
            "Low: the planner waits twice as long before building {what} and builds them last."
        ),
        (PriorityTopic::FoodReserve, PriorityLevel::High) => {
            "High: twice the usual days of food are kept on the shelves, and new fields come \
             sooner."
                .to_owned()
        }
        (PriorityTopic::FoodReserve, PriorityLevel::Low) => {
            "Low: half the usual days of food are kept on the shelves, and new fields wait \
             longer."
                .to_owned()
        }
        (PriorityTopic::FuelReserve, PriorityLevel::High) => {
            "High: homes keep twice the usual winter fuel reserve.".to_owned()
        }
        (PriorityTopic::FuelReserve, PriorityLevel::Low) => {
            "Low: homes keep half the usual winter fuel reserve.".to_owned()
        }
        (PriorityTopic::Coats, PriorityLevel::High) => {
            "High: the tailors stock a coat for every resident without one.".to_owned()
        }
        (PriorityTopic::Coats, PriorityLevel::Low) => {
            "Low: the tailors make coats only when someone orders one.".to_owned()
        }
        (PriorityTopic::BoatOrders, PriorityLevel::High) => {
            "High: the makers work for twice the boat's open order.".to_owned()
        }
        (PriorityTopic::BoatOrders, PriorityLevel::Low) => {
            "Low: nobody works for the boat's orders; the boat still buys what is offered."
                .to_owned()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_normal_keeps_the_planner_order() {
        let due = vec![
            (PriorityLevel::Normal, "beds"),
            (PriorityLevel::Normal, "field"),
            (PriorityLevel::Normal, "storage"),
        ];
        assert_eq!(order_projects(due), vec!["beds", "field", "storage"]);
    }

    #[test]
    fn a_high_priority_goes_first_and_a_low_one_last() {
        let due = vec![
            (PriorityLevel::Low, "beds"),
            (PriorityLevel::Normal, "field"),
            (PriorityLevel::High, "storage"),
            (PriorityLevel::Normal, "workplace"),
        ];
        assert_eq!(
            order_projects(due),
            vec!["storage", "field", "workplace", "beds"]
        );
    }

    #[test]
    fn levels_scale_waiting_and_reserves() {
        assert_eq!(sustain(PriorityLevel::Normal, 3), 3);
        assert_eq!(sustain(PriorityLevel::High, 3), 1);
        assert_eq!(sustain(PriorityLevel::Low, 3), 6);
        assert_eq!(scale(PriorityLevel::Normal, 5), 5);
        assert_eq!(scale(PriorityLevel::High, 5), 10);
        assert_eq!(scale(PriorityLevel::Low, 5), 2);
        assert_eq!(scale(PriorityLevel::Low, 0), 0);
    }
}
