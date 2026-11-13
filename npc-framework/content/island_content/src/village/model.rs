//! Authoritative island content, registered only in the island village.
use bevy_ecs::prelude::*;
use bevy_reflect::Reflect;
use npc_framework::kernel::hash::SimHashResource;
use npc_framework::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const MAX_DIARY_PAGES: usize = 256;
pub const MAX_DIARY_BYTES: usize = 1_048_576;
pub const DAYS_PER_SEASON: u64 = 7;
pub const NPC_SEED_COINS: u32 = 200;
/// Holdings above this pay the daily proportional levy (regional profile).
pub const LEVY_THRESHOLD: u32 = NPC_SEED_COINS;
/// Share of the holdings above [`LEVY_THRESHOLD`] moved into the fund each day.
pub const LEVY_PERCENT: u32 = 10;
/// A living resident below this is topped up to it from the fund, if it can pay.
pub const DESTITUTION_FLOOR: u32 = 40;
/// Per-resident reserve the fund keeps for floor grants before it pays the
/// equal civic dividend.
pub const FUND_RESERVE_PER_RESIDENT: u32 = 20;

/// Immutable presentation/perception construction data, like the incumbent explorer actor base.
#[derive(Component, Clone, Debug, Serialize, Deserialize)]
pub struct VillageSight(pub npc_framework::perception::Sensors);

/// A profession's identity.
///
/// The named variants are the shipped trades (and the Actor Testbed's fixed
/// table); [`Role::Trade`] is any other trade a world's catalog defines,
/// identified by its data key. In data and saves every role is its key
/// string (`"farmer"`, `"water_keeper"`, `"potter"`), so a catalog can add a
/// trade without code. Living-world logic reads behaviour from the
/// profession's catalog entry and never branches on a named trade.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Reflect)]
pub enum Role {
    Visitor,
    Farmer,
    Miller,
    Baker,
    WaterKeeper,
    Woodcutter,
    Carpenter,
    Miner,
    Smith,
    Innkeeper,
    Hunter,
    Tanner,
    Tailor,
    Fisher,
    /// A trade only the world's catalog defines, by its data key.
    Trade(&'static str),
}

/// Data keys of the named roles, in declaration order.
const NAMED_ROLES: [(Role, &str); 14] = [
    (Role::Visitor, "visitor"),
    (Role::Farmer, "farmer"),
    (Role::Miller, "miller"),
    (Role::Baker, "baker"),
    (Role::WaterKeeper, "water_keeper"),
    (Role::Woodcutter, "woodcutter"),
    (Role::Carpenter, "carpenter"),
    (Role::Miner, "miner"),
    (Role::Smith, "smith"),
    (Role::Innkeeper, "innkeeper"),
    (Role::Hunter, "hunter"),
    (Role::Tanner, "tanner"),
    (Role::Tailor, "tailor"),
    (Role::Fisher, "fisher"),
];

/// Interned keys of data-only trades. The pool only grows and holds each
/// distinct key once; a role compares, orders and hashes by its key string,
/// never by where it sits here, so the pool cannot affect a simulation.
static TRADE_KEYS: std::sync::Mutex<std::collections::BTreeSet<&'static str>> =
    std::sync::Mutex::new(std::collections::BTreeSet::new());

impl Role {
    /// The role named by a data key: a named variant, or a data-only trade.
    pub fn from_key(key: &str) -> Self {
        if let Some((role, _)) = NAMED_ROLES.iter().find(|(_, name)| *name == key) {
            return *role;
        }
        let mut pool = TRADE_KEYS
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if let Some(found) = pool.get(key) {
            return Self::Trade(found);
        }
        let leaked: &'static str = Box::leak(key.to_owned().into_boxed_str());
        pool.insert(leaked);
        Self::Trade(leaked)
    }
    /// The role's data key (`"water_keeper"`, `"potter"`).
    pub fn key(self) -> &'static str {
        match self {
            Self::Trade(key) => key,
            named => NAMED_ROLES
                .iter()
                .find(|(role, _)| *role == named)
                .map_or("visitor", |(_, key)| key),
        }
    }
    /// A data-only trade, which no fixed table knows.
    pub fn is_data_only(self) -> bool {
        matches!(self, Self::Trade(_))
    }
}

impl Serialize for Role {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.key())
    }
}

impl<'de> Deserialize<'de> for Role {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let key = String::deserialize(deserializer)?;
        let valid = !key.is_empty()
            && key
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
        if !valid {
            return Err(serde::de::Error::custom(format!(
                "`{key}` is not a trade key (lower-case letters, digits and `_`)"
            )));
        }
        Ok(Self::from_key(&key))
    }
}

impl Role {
    pub const RESIDENTS: [Self; 9] = [
        Self::Farmer,
        Self::Miller,
        Self::Baker,
        Self::WaterKeeper,
        Self::Woodcutter,
        Self::Carpenter,
        Self::Miner,
        Self::Smith,
        Self::Innkeeper,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::Visitor => "visitor",
            Self::Farmer => "farmer",
            Self::Miller => "miller",
            Self::Baker => "baker",
            Self::WaterKeeper => "well keeper",
            Self::Woodcutter => "woodcutter",
            Self::Carpenter => "carpenter",
            Self::Miner => "miner",
            Self::Smith => "blacksmith",
            Self::Innkeeper => "innkeeper",
            Self::Hunter => "hunter",
            Self::Tanner => "tanner",
            Self::Tailor => "tailor",
            Self::Fisher => "fisher",
            Self::Trade(key) => key,
        }
    }
    pub fn person(self) -> &'static str {
        match self {
            Self::Visitor => "Traveller",
            Self::Farmer => "Ada",
            Self::Miller => "Rowan",
            Self::Baker => "Mira",
            Self::WaterKeeper => "Nell",
            Self::Woodcutter => "Oak",
            Self::Carpenter => "Jun",
            Self::Miner => "Bram",
            Self::Smith => "Ivo",
            Self::Innkeeper => "Tess",
            Self::Hunter => "Rhea",
            Self::Tanner => "Sage",
            Self::Tailor => "Lina",
            Self::Fisher => "Finn",
            Self::Trade(_) => "Worker",
        }
    }
    /// Hash tag of a named role (its declaration index); every data-only
    /// trade shares the next one and hashes its key after it.
    pub fn tag(self) -> u8 {
        match self {
            Self::Trade(_) => 14,
            named => NAMED_ROLES
                .iter()
                .position(|(role, _)| *role == named)
                .map_or(0, |index| index as u8),
        }
    }
    /// Feeds the role into a state hash: the tag alone for a named role (as
    /// before data-only trades existed), the tag and key for a data trade.
    pub fn sim_hash(self, h: &mut Fnv64) {
        h.u8(self.tag());
        if let Self::Trade(key) = self {
            h.str(key);
        }
    }
}
#[derive(Component, Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Reflect)]
#[reflect(Component)]
pub struct Villager {
    pub role: Role,
    pub name: String,
    pub home: SimPos,
    pub workplace: SimId,
    pub base_speed: i32,
    pub next_decision: u64,
    pub resting_until: u64,
    pub rest_rate: i32,
    pub life: u32,
}
impl SimHash for Villager {
    const NAME: &'static str = "island_villager";
    fn sim_hash(&self, h: &mut Fnv64) {
        self.role.sim_hash(h);
        h.str(&self.name);
        self.home.sim_hash(h);
        h.u32(self.workplace.0);
        h.i32(self.base_speed);
        h.u64(self.next_decision);
        h.u64(self.resting_until);
        h.i32(self.rest_rate);
        h.u32(self.life);
    }
}
#[derive(
    Component, Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Reflect,
)]
#[reflect(Component)]
pub struct Wallet {
    pub coins: u32,
}
impl SimHash for Wallet {
    const NAME: &'static str = "island_wallet";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.u32(self.coins);
    }
}

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Reflect)]
#[reflect(Component)]
pub struct Vitality {
    pub health: i32,
    pub starved_ticks: u32,
    pub dehydrated_ticks: u32,
    pub died_at: Option<u64>,
    pub exhaustion_ticks: u32,
}
impl Default for Vitality {
    fn default() -> Self {
        Self {
            health: 100_000,
            starved_ticks: 0,
            dehydrated_ticks: 0,
            died_at: None,
            exhaustion_ticks: 0,
        }
    }
}
impl SimHash for Vitality {
    const NAME: &'static str = "island_vitality";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.i32(self.health);
        h.u32(self.starved_ticks);
        h.u32(self.dehydrated_ticks);
        h.opt(self.died_at.as_ref(), |h, t| h.u64(*t));
        h.u32(self.exhaustion_ticks);
    }
}

#[derive(Component, Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Reflect)]
#[reflect(Component)]
pub struct VillagePlace {
    pub role: Role,
    pub label: String,
}
impl SimHash for VillagePlace {
    const NAME: &'static str = "island_place";
    fn sim_hash(&self, h: &mut Fnv64) {
        self.role.sim_hash(h);
        h.str(&self.label);
    }
}

#[derive(Component, Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Reflect)]
#[reflect(Component)]
pub struct ResourcePatch {
    pub kind: String,
    pub remaining: u32,
    pub capacity: u32,
    pub replenish_at: u64,
    pub gather_ticks: u32,
}
impl SimHash for ResourcePatch {
    const NAME: &'static str = "island_resource_patch";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.str(&self.kind);
        h.u32(self.remaining);
        h.u32(self.capacity);
        h.u64(self.replenish_at);
        h.u32(self.gather_ticks);
    }
}

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Reflect)]
#[reflect(Component)]
#[component(storage = "SparseSet")]
pub struct Gathering {
    pub patch: SimId,
    pub ends: u64,
}
impl SimHash for Gathering {
    const NAME: &'static str = "island_gathering";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.u32(self.patch.0);
        h.u64(self.ends);
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub struct DiaryPage {
    pub tick: u64,
    pub title: String,
    pub body: String,
}
fn hash_pages(pages: &[DiaryPage], h: &mut Fnv64) {
    h.u32(pages.len() as u32);
    for p in pages {
        h.u64(p.tick);
        h.str(&p.title);
        h.str(&p.body);
    }
}
#[derive(Component, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Reflect)]
#[reflect(Component)]
pub struct VillageDiary {
    pub pages: Vec<DiaryPage>,
}
impl SimHash for VillageDiary {
    const NAME: &'static str = "island_diary";
    fn sim_hash(&self, h: &mut Fnv64) {
        hash_pages(&self.pages, h);
    }
}

#[derive(Component, Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Reflect)]
#[reflect(Component)]
pub struct DroppedDiary {
    pub author: SimId,
    pub author_name: String,
    pub life: u32,
    pub died_at: u64,
    pub pages: Vec<DiaryPage>,
    pub sealed: bool,
}
impl SimHash for DroppedDiary {
    const NAME: &'static str = "island_dropped_diary";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.u32(self.author.0);
        h.str(&self.author_name);
        h.u32(self.life);
        h.u64(self.died_at);
        hash_pages(&self.pages, h);
        h.u8(u8::from(self.sealed));
    }
}
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Reflect)]
#[reflect(Component)]
pub struct CoinPurse {
    pub coins: u32,
}
impl SimHash for CoinPurse {
    const NAME: &'static str = "island_coin_purse";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.u32(self.coins);
    }
}

#[derive(Resource, Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub struct VillageState {
    pub played: SimId,
    pub agent_alias: String,
    pub life: u32,
    pub fund: u32,
    pub last_dividend_day: u64,
    pub produced: BTreeMap<String, u64>,
    pub transactions: u64,
}
impl SimHashResource for VillageState {
    const NAME: &'static str = "island_village";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.u32(self.played.0);
        h.str(&self.agent_alias);
        h.u32(self.life);
        h.u32(self.fund);
        h.u64(self.last_dividend_day);
        h.u32(self.produced.len() as u32);
        for (k, v) in &self.produced {
            h.str(k);
            h.u64(*v);
        }
        h.u64(self.transactions);
    }
    fn snapshot_fns() -> Option<npc_framework::snapshot::ResourceSnapshotFns> {
        Some(npc_framework::snapshot::serde_resource_fns::<Self>())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Reflect)]
#[serde(rename_all = "snake_case")]
pub enum Season {
    Spring,
    Summer,
    Autumn,
    Winter,
}
pub fn calendar(world: &World) -> (u64, Season, bool) {
    let tick = world.resource::<SimClock>().tick;
    calendar_on(world, tick)
}
/// Days in one season of this world: data in the regional profile
/// (`economy.tuning.days_per_season`), [`DAYS_PER_SEASON`] elsewhere.
pub fn days_per_season(world: &World) -> u64 {
    super::frontier::economy_spec::tuning(world)
        .map_or(DAYS_PER_SEASON, |tuning| tuning.days_per_season.max(1))
}
/// Calendar day of tick 0. Older profiles: 19 when needs-driven, else 0. The
/// regional profile reads `economy.tuning.founding_day`.
pub fn first_day(world: &World) -> u64 {
    if !super::needs_driven(world) {
        return 0;
    }
    match super::frontier::economy_spec::tuning(world) {
        Some(tuning) if tuning.founding_day >= 0 => tuning.founding_day as u64,
        Some(tuning) => founding_day(tuning.days_per_season.max(1)),
        None => founding_day(DAYS_PER_SEASON),
    }
}
/// The calendar of this world at any tick, with its own year length.
pub fn calendar_on(world: &World, tick: u64) -> (u64, Season, bool) {
    let config = world.resource::<SimConfig>();
    if super::frontier::economy_spec::tuning(world).is_none() {
        return calendar_at(tick, config, super::needs_driven(world));
    }
    calendar_with(tick, config, first_day(world), days_per_season(world))
}
/// First calendar day of a needs-driven world: two days before winter.
pub fn founding_day(season_days: u64) -> u64 {
    (3 * season_days).saturating_sub(2).max(2 * season_days)
}
/// [`calendar_at`] for a year of `4 * season_days` days whose tick 0 falls on
/// calendar day `first_day`.
pub fn calendar_with(
    tick: u64,
    config: &SimConfig,
    first_day: u64,
    season_days: u64,
) -> (u64, Season, bool) {
    let season_days = season_days.max(1);
    let day = tick / config.day_length.max(1) + first_day;
    (day, season_of(day, season_days), config.is_night(tick))
}
/// Season of a calendar day.
pub fn season_of(day: u64, season_days: u64) -> Season {
    match day / season_days.max(1) % 4 {
        0 => Season::Spring,
        1 => Season::Summer,
        2 => Season::Autumn,
        _ => Season::Winter,
    }
}
/// Shared simulation/observer calendar; Living starts in late autumn.
pub fn calendar_at(tick: u64, config: &SimConfig, needs_driven: bool) -> (u64, Season, bool) {
    let day = tick / config.day_length.max(1) + if needs_driven { 19 } else { 0 };
    let season = match day / DAYS_PER_SEASON % 4 {
        0 => Season::Spring,
        1 => Season::Summer,
        2 => Season::Autumn,
        _ => Season::Winter,
    };
    (day, season, config.is_night(tick))
}
pub fn living(world: &World, e: Entity) -> bool {
    world
        .get::<Vitality>(e)
        .is_some_and(|v| v.died_at.is_none())
}
