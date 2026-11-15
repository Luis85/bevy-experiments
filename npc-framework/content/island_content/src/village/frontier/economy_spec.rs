//! Versioned, validated economy data for the regional island profile.
//!
//! Goods, recipes, professions, resource patches, founding endowments and
//! tuning live in [`EconomySpec`], the `economy` block of
//! [`super::LivingWorldSpec`]. The block is validated before registration,
//! copied into the save's immutable construction data with the world spec and
//! hashed with it. [`EconomyRegistry`] is the frozen lookup built from it.
//! `Role` stays a closed enum: it is the identity of a profession, while every
//! per-profession property is data.
use crate::village::Role;
use crate::village::catalog::{
    BAKE, BERRIES, BOWL, CARVE, COOK, COOK_FISH, COOKED_MEAL, FIBER, FISH, FISHING_ROD, FLOUR,
    FORGE_AXE, FORGE_HOE, FORGE_PICKAXE, FORGE_SPEAR, GRAIN, HIDE, HOE, INN, LEATHER, MAKE_ROD,
    MILL, RAW_MEAT, SAW, SMELT, SPEAR, SPIN, TAILOR, TAN, THREAD, TOOL, WARM_CLOTHES, WATER,
    WEAVE_COAT,
};
use bevy_ecs::prelude::*;
use npc_framework::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Mutex, OnceLock, PoisonError};

pub const ECONOMY_VERSION: u32 = 1;
/// Item kinds registered by the engine content rather than by this block. A
/// good without an `item` section must be one of these.
pub const BUILTIN_ITEM_KINDS: [&str; 8] = [
    "axe", "bread", "hammer", "ingot", "log", "ore", "pickaxe", "plank",
];
/// Residents founded per settlement; names, homes and integrity checks use it.
pub const FOUNDERS_PER_SETTLEMENT: usize = 14;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GoodTag {
    Food,
    Tool,
    Fuel,
    Clothing,
    /// Every resident accepts and may buy this good, whatever their trade.
    Staple,
    /// What residents drink (the island's water).
    Drink,
    /// Raw building material (felled timber).
    Timber,
    /// Sawn building material.
    Lumber,
    /// Smelted metal.
    Metal,
    /// What metal is smelted from.
    Ore,
    /// What a hunt yields to eat (cooked or sold).
    Meat,
    /// What a hunt yields to tan.
    Hide,
    /// Tanned hide.
    Leather,
    /// Spun fibre.
    Yarn,
    /// What yarn is spun from.
    Fiber,
    /// The baked staple food.
    Baked,
    /// The cooked meal.
    Cooked,
    /// Food foraged in the wild.
    Foraged,
    /// The crop sown in the fields and milled.
    Grain,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ItemSpec {
    pub weight_g: u32,
    /// 1 means the item does not stack.
    #[serde(default = "one")]
    pub max_stack: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub equip_slot: Option<String>,
}
fn one() -> u32 {
    1
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GoodDef {
    pub key: String,
    /// Lower-case noun shown in job summaries, for example `logs`.
    pub display: String,
    pub base_price: u32,
    /// Hunger restored by eating one unit; requires the `food` tag.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub food_value: Option<i32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<GoodTag>,
    /// Uses before a crafted or granted unit breaks.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub durability: Option<u32>,
    /// The tool good that speeds gathering this good from a patch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gather_tool: Option<String>,
    /// Without a usable `gather_tool` the good cannot be gathered at all
    /// (a fishing rod for fish), instead of slowly by hand.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub gather_tool_required: bool,
    /// Mean life in days of a unit in a pack (stores keep it
    /// `tuning.storage_shelf_percent` as long); absent: it keeps for ever.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shelf_life_days: Option<u32>,
    /// Item kind registered for this good; omit for an engine item kind.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item: Option<ItemSpec>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stack {
    pub good: String,
    pub count: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkillRequirement {
    pub name: String,
    /// Recipe: roll modifier. Profession: minimum effective level.
    pub level: i32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecipeSpec {
    pub key: String,
    /// Sentence-case job label, for example `Mill flour`.
    pub display: String,
    pub station: String,
    pub inputs: Vec<Stack>,
    pub outputs: Vec<Stack>,
    pub duration: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skill: Option<SkillRequirement>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KeepDef {
    pub good: String,
    pub count: u32,
    /// Whether the plan also reserves working capital for this good's inputs.
    #[serde(default = "yes")]
    pub reserve_inputs: bool,
}
fn yes() -> bool {
    true
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProfessionDef {
    pub role: Role,
    /// Workstation kind of this profession's workplace.
    pub station: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub outputs: Vec<String>,
    /// Goods this profession buys besides the staples everyone accepts.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub wants: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub recipes: Vec<String>,
    /// Gathering tool the profession keeps one of and is founded with.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool: Option<String>,
    /// Tool without which the profession cannot work at all.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub required_tool: Option<String>,
    /// Standing stock the profession orders through the funded plan.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keeps: Vec<KeepDef>,
    /// Aptitude: the trades a founder of this profession may ever take.
    pub aptitudes: Vec<Role>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skill: Option<SkillRequirement>,
    /// Actor template of a founder, and of a founder who may take any trade.
    pub template: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flexible_template: Option<String>,
    /// A settlement never gives up its last worker of an essential profession.
    #[serde(default)]
    pub essential: bool,
    /// Goods a founder starts with, besides `seed.common` and the tool.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub endowment: Vec<Stack>,
    /// A service trade (the inn): one keeper per workplace earns what guests
    /// pay. It is staffed when its building has nobody, not by comparing
    /// output margins, and its keeper stays while the building stands.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service: Option<ServiceDef>,
    /// The trade hunts: with this tool, for these yields (living world).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hunt: Option<HuntDef>,
    /// The trade builds homes (commissions and hired homestead work).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub builds: bool,
    /// The trade sows and harvests the fields.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub sows: bool,
    /// The trade's name as players read it (`tailor`, `clothier`, ...);
    /// empty: the role's own name.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub display: String,
    /// The behaviour tree its workers run (S3.1); none: the default tree.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tree: Option<String>,
}

/// What a hunting trade hunts with and what one hunt yields.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HuntDef {
    pub tool: String,
    pub yields: Vec<Stack>,
    /// The wildlife kind it hunts (`deer`).
    pub prey: String,
}

/// What a service trade's keeper is assured of.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServiceDef {
    /// Coins a day the civic fund tops the keeper's takings up to, while the
    /// building is the only one of its trade in its settlement and the fund
    /// may spend them.
    pub wage_floor: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PatchDef {
    pub good: String,
    /// Offset from the settlement centre in millimetres.
    pub dx: i32,
    pub dz: i32,
    /// Place the patch at this profession's workplace instead of the offset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at_site: Option<Role>,
    /// Settlement biomes that receive the patch; empty means every settlement.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub biomes: Vec<super::Biome>,
    pub capacity: u32,
    pub gather_ticks: u32,
    /// A finite patch never regrows.
    #[serde(default)]
    pub finite: bool,
    /// Units regrown per 600-tick round at full biomass and soil.
    #[serde(default)]
    pub regrowth: u32,
    /// Regrowth per round in winter, when it differs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub winter_regrowth: Option<u32>,
    /// Biomass (milli) lost per harvested unit; half of it becomes disturbance.
    #[serde(default)]
    pub harvest_loss: u32,
    /// Soil (milli) lost per harvested unit.
    #[serde(default)]
    pub soil_loss: u32,
    /// The patch lies at the water: on the dry cell beside water nearest to
    /// its authored spot (a fishing ground).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub shore: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FounderDef {
    pub role: Role,
    /// A flexible founder may take any trade.
    #[serde(default)]
    pub flexible: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SeedSpec {
    pub coins: u32,
    /// Goods every founder starts with.
    pub common: Vec<Stack>,
    /// The fourteen founders of each settlement, in home order.
    pub founders: Vec<FounderDef>,
    /// Finished foods a founder may prefer; the seed picks one per resident.
    pub food_preferences: Vec<String>,
    /// Seeded band: each common good gets 0..=this many extra units.
    pub endowment_bonus_max: u32,
    /// Seeded extra aptitudes granted to each founder who is not flexible.
    pub extra_aptitudes: u32,
}
impl Default for SeedSpec {
    fn default() -> Self {
        let mut founders: Vec<_> = ROLES
            .into_iter()
            .map(|role| FounderDef {
                role,
                flexible: matches!(role, Role::Innkeeper | Role::Fisher),
            })
            .collect();
        founders.push(FounderDef {
            role: Role::Woodcutter,
            flexible: true,
        });
        Self {
            coins: 200,
            common: stacks(&[("bread", 2), (WATER, 2), ("log", 2)]),
            founders,
            food_preferences: vec![COOKED_MEAL.into(), "bread".into(), "bread".into()],
            endowment_bonus_max: 1,
            extra_aptitudes: 1,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct EconomyTuning {
    /// Holdings above this pay the daily proportional levy.
    pub levy_threshold: u32,
    pub levy_percent: u32,
    /// A living resident below this is topped up from the fund, if it can pay.
    pub destitution_floor: u32,
    /// Per-resident reserve the fund keeps before paying the equal dividend.
    pub fund_reserve_per_resident: u32,
    /// A vacancy must out-earn the current job by this much before it counts.
    pub margin_gap_permille: i64,
    pub margin_persist_ticks: u64,
    /// Switching cooldown once a margin gap has persisted.
    pub margin_cooldown_ticks: u64,
    pub switch_cooldown_ticks: u64,
    /// Gathering duration with and without the matching tool, percent of base.
    pub tool_gather_percent: u32,
    pub bare_gather_percent: u32,
    /// Building stage time with a usable builder's tool, percent.
    pub build_tool_percent: u32,
    /// Building stage time without one, percent.
    pub build_bare_percent: u32,
    /// How much longer goods keep in a store than in a pack, percent.
    pub storage_shelf_percent: u32,
    /// Work duration at zero and at full morale, percent of base.
    pub work_pace_slowest_percent: u32,
    pub work_pace_fastest_percent: u32,
    /// Below this Warmth a resident loses `cold_health_drain` health per tick.
    pub cold_health_threshold: i32,
    pub cold_health_drain: i32,
    /// Hearth logs one home burns per winter day (two outside winter).
    pub winter_hearth_lightings: u32,
    /// A learned market price never exceeds this percentage of the base price.
    pub price_ceiling_percent: u32,
    /// Days before winter from which every home's steward stocks hearth fuel.
    pub winter_fuel_lead_days: u64,
    /// Hearth fuel a steward keeps in reserve in and shortly before winter.
    pub winter_fuel_reserve: u32,
    /// A resident does not return to a profession left less than this long ago,
    /// unless an essential workplace stands empty.
    pub profession_return_ticks: u64,
    /// A seat somebody just left is not refilled for this long.
    pub vacated_seat_ticks: u64,
    /// Units of each good a producer keeps in hand instead of storing them.
    pub depot_working_stock: u32,
    /// Smallest surplus of one good worth a walk to a store.
    pub depot_deposit_min: u32,
    /// A stall holding fewer units of a good than this is restocked from storage.
    pub stall_restock_level: u32,
    /// Most units a resident carries to a site or a stall in one trip.
    pub haul_trip_units: u32,
    /// How long a hauler's claim on store goods or a site order lasts.
    pub haul_reservation_ticks: u64,
    /// Builder ticks worked on a site in one go.
    pub build_stage_ticks: u32,
    /// Coins the fund keeps back from civic purchasing for destitution grants:
    /// on a bad day the grants exceed the levy, and the fund must cover them.
    pub civic_reserve: u32,
    /// Extra share of the open material cost the fund withholds from the dividend.
    pub civic_budget_slack_percent: u32,
    /// Ticks between two civic reviews of one settlement.
    pub planner_review_ticks: u64,
    /// Consecutive reviews a shortage must last before the planner builds.
    pub planner_sustain_reviews: u32,
    /// Unstored surplus units in a settlement that count as storage overflow.
    pub planner_storage_overflow_units: u32,
    /// Unmet units of a profession's goods that count as unmet demand.
    pub planner_demand_units: u32,
    /// How far from the settlement centre the planner looks for a site, in cells.
    pub planner_search_radius_cells: u32,
    /// A civic site that is not finished after this long is cancelled.
    pub planner_project_timeout_ticks: u64,
    /// Days in one season; a year has four. Ages are counted in seasons, so a
    /// shorter year also shortens every life.
    pub days_per_season: u64,
    /// The fund also keeps this many days of its recent destitution grants
    /// back from civic purchasing, when that is more than `civic_reserve`.
    pub civic_reserve_grant_days: u32,
    /// Days of grants the fund remembers for that reserve.
    pub civic_reserve_window_days: u32,
    /// Unmet units of a field crop that count as a grain shortage.
    pub planner_crop_shortage_units: u32,
    /// Calendar day the world is founded on; day 0 is the first day of
    /// spring. -1 founds it two days before the first winter.
    pub founding_day: i64,
    /// When open building sites need more than the fund may spend, every
    /// resident pays up to this share of the coins held above the
    /// destitution floor into the fund, earmarked for building.
    pub building_levy_percent: u32,
    /// When the fund cannot pay the destitution grants of the day, every
    /// resident pays up to this share of the coins held above the destitution
    /// floor to make up the difference.
    pub relief_levy_percent: u32,
    /// Energy a resident asleep in a bed at night gains per tick, before the
    /// ordinary decay. Sleep lasts to the end of the sleep block.
    pub sleep_energy_per_tick: u32,
    /// Energy gained per tick of a short rest at home by day.
    pub nap_energy_per_tick: u32,
    /// While a gathered good sells above its base price, each gatherer who
    /// can work keeps this many units beyond its orders. 0 switches it off.
    pub gatherer_shelf_units: u32,
    /// The lowest a price may fall, in percent of the base price (and never
    /// below one coin). 0 leaves only the one-coin floor. A safety switch:
    /// reservation prices and stock targets keep prices up by themselves.
    pub price_floor_percent: u32,
    /// Guest beds of an inn.
    pub inn_guest_beds: u32,
    /// Residents and stores do not offer a good below its reservation price:
    /// a made good at its input cost at today's prices plus
    /// `maker_margin_percent`, a raw good at `raw_floor_percent` of its base.
    pub reservation_prices: bool,
    pub maker_margin_percent: u32,
    pub raw_floor_percent: u32,
    /// Makers start no new batch of a good while the settlement's unsold
    /// stock and open offers exceed this many days of its use. 0 switches it
    /// off.
    pub stock_target_days: u32,
    /// Staples (foods, water, fuel, the winter coat) are made to a planned
    /// stock of this many days of the settlement's use, whether or not a
    /// buyer has ordered them yet (closing design: the makers work toward a
    /// stock, not only to order). Never more days than the good keeps.
    pub staple_stock_days: u32,
    /// The staple stock in the cold season and the season before it.
    pub staple_winter_stock_days: u32,
    /// Coins the civic fund may advance in one day to staple makers whose
    /// purse cannot pay for the inputs of their standing work, repaid from
    /// their sales.
    pub staple_advance_per_day: u32,
    /// The most a single maker may owe the fund for advances.
    pub staple_advance_per_maker: u32,
    /// Stock up to the island's stock target is working stock: the market
    /// lowers a price only for what is offered beyond it.
    pub stock_cover_pricing: bool,
    /// Market rounds of trade the daily use of goods that are not eaten is
    /// estimated from (eight rounds make a day).
    pub trade_window_rounds: u32,
    /// Share of each merchant-boat trade the boat pays into the civic fund.
    pub boat_duty_percent: u32,
    /// Below this Warmth a resident without a coat gets warm before anything
    /// else (the `life.freezing` condition), unless Thirst or Hunger is
    /// critical and lower still.
    pub freezing_warmth: i32,
    /// In the cold (night, winter or snow) a resident without a coat starts
    /// no outdoor errand below this Warmth; buying food, water, fuel or a
    /// coat and going home are not errands.
    pub exposure_warmth: i32,
    /// Days before winter from which a resident without a coat buys one as a
    /// priority (the `life.coat_season` condition), the civic fund paying
    /// when it cannot, and tailors stock one for everybody without.
    pub coat_lead_days: u32,
    /// A gathered good (logs, water) has a stock target like a made one, so
    /// its stock on offer up to the target is cover, not a glut. Off, only
    /// goods made in batches or grown once a season have cover.
    pub raw_stock_cover: bool,
    /// A worker whose trade has no work may leave it after this many days in
    /// it (`labor::idle_trade`); 0, the default, switches the rule off: with
    /// 2 days the 14-day audit saw 319-364 job changes against 62-95 without.
    pub idle_trade_days: u32,
    /// A tired or cold resident this many metres or more from home rents a
    /// bed at an inn that is at most half as far, when it can pay; 0 never.
    pub inn_detour_m: u32,
    /// Goods a resident has no use for (`market::no_use`) are offered for
    /// sale and carried to a store whole.
    pub unused_goods_to_market: bool,
    /// While anybody of a settlement lacks a coat, each coat maker keeps a
    /// shelf of `max(2, residents / coat_shelf_residents)` coats in every
    /// season, from its own purse; 0 switches the shelf off.
    pub coat_shelf_residents: u32,
    /// Outside the coat season a resident without a coat who cannot pay one
    /// on offer is granted the price from the fund's balance above its
    /// destitution reserve.
    pub coat_grants_any_season: bool,
    /// Floor grants do not take the coins the fund holds for open sites
    /// (`civic::fund_hold`); the relief levy raises what the floor needs
    /// beyond.
    pub grants_spare_sites: bool,
    /// Of every three founders (by identity) this many come wearing a coat;
    /// 0 founds everybody without one.
    pub founder_coats_per_three: u32,
    /// Wild animals move (`wildlife::WildlifeMotion`); off they stand where
    /// they were born.
    pub wildlife_motion: bool,
    /// Each settlement is founded with its coat makers' workplace built and
    /// each coat maker holding one batch of coat materials.
    pub found_coat_workshops: bool,
    /// Coins a night in an inn bed costs.
    pub inn_bed_price: u32,
    /// Ticks one rented night lasts.
    pub inn_bed_ticks: u64,
}
impl Default for EconomyTuning {
    fn default() -> Self {
        Self {
            levy_threshold: 200,
            levy_percent: 10,
            destitution_floor: 40,
            fund_reserve_per_resident: 20,
            margin_gap_permille: 250,
            margin_persist_ticks: 12000,
            margin_cooldown_ticks: 6000,
            switch_cooldown_ticks: 12000,
            tool_gather_percent: 75,
            bare_gather_percent: 150,
            build_tool_percent: 100,
            build_bare_percent: 150,
            storage_shelf_percent: 200,
            work_pace_slowest_percent: 115,
            work_pace_fastest_percent: 90,
            cold_health_threshold: 10_000,
            cold_health_drain: 6,
            winter_hearth_lightings: 3,
            price_ceiling_percent: 500,
            winter_fuel_lead_days: 2,
            winter_fuel_reserve: 3,
            profession_return_ticks: 48_000,
            vacated_seat_ticks: 2_400,
            depot_working_stock: 2,
            depot_deposit_min: 4,
            stall_restock_level: 4,
            haul_trip_units: 8,
            haul_reservation_ticks: 1_200,
            build_stage_ticks: 200,
            civic_reserve: 250,
            civic_budget_slack_percent: 25,
            planner_review_ticks: 1_200,
            planner_sustain_reviews: 3,
            planner_storage_overflow_units: 40,
            planner_demand_units: 6,
            planner_search_radius_cells: 36,
            planner_project_timeout_ticks: 168_000,
            days_per_season: 7,
            civic_reserve_grant_days: 3,
            civic_reserve_window_days: 7,
            planner_crop_shortage_units: 4,
            founding_day: -1,
            building_levy_percent: 5,
            relief_levy_percent: 5,
            sleep_energy_per_tick: 12,
            nap_energy_per_tick: 100,
            gatherer_shelf_units: 4,
            price_floor_percent: 0,
            inn_guest_beds: 4,
            reservation_prices: true,
            maker_margin_percent: 10,
            raw_floor_percent: 25,
            stock_target_days: 2,
            staple_stock_days: 4,
            staple_winter_stock_days: 6,
            staple_advance_per_day: 120,
            staple_advance_per_maker: 30,
            stock_cover_pricing: true,
            trade_window_rounds: 24,
            boat_duty_percent: 5,
            freezing_warmth: 30_000,
            exposure_warmth: 50_000,
            coat_lead_days: 3,
            raw_stock_cover: true,
            idle_trade_days: 0,
            inn_detour_m: 60,
            unused_goods_to_market: true,
            coat_shelf_residents: 15,
            coat_grants_any_season: true,
            grants_spare_sites: true,
            founder_coats_per_three: 2,
            wildlife_motion: true,
            found_coat_workshops: true,
            inn_bed_price: 3,
            inn_bed_ticks: 300,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct EconomySpec {
    pub version: u32,
    pub goods: Vec<GoodDef>,
    pub recipes: Vec<RecipeSpec>,
    pub professions: Vec<ProfessionDef>,
    pub patches: Vec<PatchDef>,
    /// What residents and the overseer can build.
    pub buildings: Vec<super::building_spec::BuildingDef>,
    pub seed: SeedSpec,
    pub tuning: EconomyTuning,
    /// Ages, couples, births, old age and arrivals.
    pub demography: super::life_spec::DemographySpec,
    /// Crops residents sow on fields.
    pub crops: Vec<super::life_spec::CropSpec>,
    /// Daily weather odds and rare seasonal events.
    pub weather: super::life_spec::WeatherSpec,
    /// The merchant boat: the island's only external source and sink.
    pub merchant: super::life_spec::MerchantSpec,
    /// Live mode: bonds, gifts, gatherings, goals and homestead tiers.
    pub live: super::life_spec::LiveSpec,
    /// Props the overseer may place with the Shape tools.
    pub props: Vec<super::props::PropDef>,
}

/// The thirteen resident professions, in founding order.
pub const ROLES: [Role; 13] = [
    Role::Farmer,
    Role::Miller,
    Role::Baker,
    Role::WaterKeeper,
    Role::Woodcutter,
    Role::Carpenter,
    Role::Miner,
    Role::Smith,
    Role::Innkeeper,
    Role::Hunter,
    Role::Tanner,
    Role::Tailor,
    Role::Fisher,
];

fn stacks(entries: &[(&str, u32)]) -> Vec<Stack> {
    entries
        .iter()
        .map(|(good, count)| Stack {
            good: (*good).into(),
            count: *count,
        })
        .collect()
}
fn names(entries: &[&str]) -> Vec<String> {
    entries.iter().map(|name| (*name).into()).collect()
}

struct Good<'a> {
    key: &'a str,
    display: &'a str,
    price: u32,
    food: Option<i32>,
    tags: &'a [GoodTag],
    durability: Option<u32>,
    gather_tool: Option<&'a str>,
    /// `(weight, max stack, item tag, equip slot)`
    item: Option<(u32, u32, Option<&'a str>, Option<&'a str>)>,
}
impl Good<'_> {
    fn build(self) -> GoodDef {
        GoodDef {
            key: self.key.into(),
            display: self.display.into(),
            base_price: self.price,
            food_value: self.food,
            tags: self.tags.to_vec(),
            durability: self.durability,
            gather_tool: self.gather_tool.map(Into::into),
            gather_tool_required: false,
            shelf_life_days: None,
            item: self.item.map(|(weight_g, max_stack, tag, slot)| ItemSpec {
                weight_g,
                max_stack,
                tags: tag.into_iter().map(Into::into).collect(),
                equip_slot: slot.map(Into::into),
            }),
        }
    }
}
/// `_limit` was the deleted `stock_limit` (review fix S2.5); the tables
/// below keep their old column for readability.
const fn good<'a>(key: &'a str, display: &'a str, price: u32, _limit: u32) -> Good<'a> {
    Good {
        key,
        display,
        price,
        food: None,
        tags: &[],
        durability: None,
        gather_tool: None,
        item: None,
    }
}

fn default_goods() -> Vec<GoodDef> {
    use GoodTag::{
        Baked, Clothing, Cooked, Drink, Fiber, Food, Foraged, Fuel, Grain, Hide, Leather, Lumber,
        Meat, Metal, Ore, Staple, Timber, Tool, Yarn,
    };
    let item = |weight, stack| Some((weight, stack, None, None));
    vec![
        Good {
            tags: &[Tool],
            durability: Some(120),
            ..good("axe", "axes", 18, 3)
        },
        Good {
            food: Some(80_000),
            tags: &[Food, Staple, Baked],
            ..good("bread", "bread", 6, 24)
        },
        Good {
            tags: &[Tool],
            durability: Some(60),
            ..good("hammer", "hammers", 18, 3)
        },
        Good {
            tags: &[Metal],
            ..good("ingot", "ingots", 6, 12)
        },
        Good {
            tags: &[Fuel, Timber],
            gather_tool: Some("axe"),
            ..good("log", "logs", 3, 20)
        },
        Good {
            tags: &[Ore],
            gather_tool: Some("pickaxe"),
            ..good("ore", "ore", 3, 20)
        },
        Good {
            tags: &[Tool],
            durability: Some(120),
            ..good("pickaxe", "pickaxes", 18, 3)
        },
        Good {
            tags: &[Lumber],
            ..good("plank", "planks", 2, 20)
        },
        Good {
            food: Some(20_000),
            tags: &[Food, Staple, Foraged],
            item: item(100, 40),
            ..good(BERRIES, "berries", 2, 40)
        },
        Good {
            item: item(200, 20),
            ..good(BOWL, "bowls", 8, 6)
        },
        Good {
            food: Some(90_000),
            tags: &[Food, Staple, Cooked],
            item: item(500, 10),
            ..good(COOKED_MEAL, "cooked meals", 9, 24)
        },
        Good {
            tags: &[Fiber],
            item: item(100, 40),
            ..good(FIBER, "fiber", 1, 40)
        },
        Good {
            item: item(500, 20),
            gather_tool: Some(FISHING_ROD),
            ..good(FISH, "fish", 4, 20)
        },
        Good {
            tags: &[Tool],
            item: Some((800, 1, Some("tool:fishing"), None)),
            durability: Some(60),
            ..good(FISHING_ROD, "fishing rods", 12, 4)
        },
        Good {
            item: item(250, 50),
            ..good(FLOUR, "flour", 6, 40)
        },
        Good {
            tags: &[Grain],
            gather_tool: Some(HOE),
            item: item(250, 50),
            ..good(GRAIN, "grain", 3, 40)
        },
        Good {
            tags: &[Hide],
            item: item(500, 20),
            ..good(HIDE, "hides", 4, 16)
        },
        Good {
            tags: &[Tool],
            durability: Some(120),
            item: Some((1200, 1, Some("tool:hoe"), None)),
            ..good(HOE, "hoes", 14, 3)
        },
        Good {
            tags: &[Leather],
            item: item(400, 20),
            ..good(LEATHER, "leather", 10, 16)
        },
        Good {
            tags: &[Meat],
            item: item(500, 20),
            ..good(RAW_MEAT, "raw meat", 5, 16)
        },
        Good {
            tags: &[Tool],
            item: Some((1500, 1, Some("tool:hunting"), None)),
            durability: Some(20),
            ..good(SPEAR, "hunting spears", 10, 8)
        },
        Good {
            tags: &[Yarn],
            item: item(100, 40),
            ..good(THREAD, "thread", 4, 24)
        },
        Good {
            tags: &[Clothing, Staple],
            durability: Some(96_000),
            item: Some((1500, 1, None, Some("body"))),
            ..good(WARM_CLOTHES, "warm clothes", 34, 4)
        },
        Good {
            tags: &[Staple, Drink],
            item: item(1000, 20),
            ..good(WATER, "clean water", 1, 40)
        },
    ]
    .into_iter()
    .map(Good::build)
    .map(|mut good| {
        // No rod, no fishing (review fix S2.2).
        good.gather_tool_required = good.key == FISH;
        // Food spoils (S2.4): days of mean life in a pack.
        good.shelf_life_days = match good.key.as_str() {
            "bread" => Some(20),
            COOKED_MEAL => Some(10),
            BERRIES => Some(14),
            _ => None,
        };
        good
    })
    .collect()
}

fn recipe(
    key: &str,
    display: &str,
    station: &str,
    inputs: &[(&str, u32)],
    duration: u32,
    output: (&str, u32),
) -> RecipeSpec {
    RecipeSpec {
        key: key.into(),
        display: display.into(),
        station: station.into(),
        inputs: stacks(inputs),
        outputs: stacks(&[output]),
        duration,
        tool: None,
        skill: None,
    }
}

fn default_recipes() -> Vec<RecipeSpec> {
    vec![
        recipe(MILL, "Mill flour", MILL, &[(GRAIN, 1)], 140, (FLOUR, 1)),
        recipe(
            BAKE,
            "Bake bread",
            BAKE,
            &[(FLOUR, 2), (WATER, 1), ("log", 1)],
            240,
            ("bread", 4),
        ),
        recipe(SAW, "Saw planks", SAW, &[("log", 1)], 160, ("plank", 4)),
        recipe(CARVE, "Carve bowls", SAW, &[("plank", 2)], 200, (BOWL, 1)),
        recipe(
            SMELT,
            "Smelt ingots",
            SMELT,
            &[("ore", 2), ("log", 1)],
            240,
            ("ingot", 2),
        ),
        recipe(
            TOOL,
            "Forge hammers",
            SMELT,
            &[("ingot", 2), ("plank", 1)],
            240,
            ("hammer", 1),
        ),
        recipe(
            FORGE_SPEAR,
            "Make hunting spears",
            SMELT,
            &[("ingot", 1), ("plank", 1)],
            200,
            (SPEAR, 1),
        ),
        recipe(
            COOK,
            "Cook meals",
            BAKE,
            &[(RAW_MEAT, 2), (WATER, 1), ("log", 1)],
            200,
            (COOKED_MEAL, 2),
        ),
        recipe(
            TAN,
            "Tan leather",
            TAN,
            &[(HIDE, 2), (WATER, 1), (BOWL, 1)],
            240,
            (LEATHER, 2),
        ),
        recipe(SPIN, "Spin thread", TAILOR, &[(FIBER, 2)], 140, (THREAD, 1)),
        recipe(
            TAILOR,
            "Sew warm clothes",
            TAILOR,
            &[(LEATHER, 2), (THREAD, 2)],
            320,
            (WARM_CLOTHES, 1),
        ),
        recipe(
            FORGE_AXE,
            "Forge axes",
            SMELT,
            &[("ingot", 1), ("plank", 1)],
            220,
            ("axe", 1),
        ),
        recipe(
            FORGE_PICKAXE,
            "Forge pickaxes",
            SMELT,
            &[("ingot", 2), ("plank", 1)],
            240,
            ("pickaxe", 1),
        ),
        recipe(
            FORGE_HOE,
            "Forge hoes",
            SMELT,
            &[("ingot", 1), ("plank", 1)],
            200,
            (HOE, 1),
        ),
        recipe(
            MAKE_ROD,
            "Make fishing rods",
            SAW,
            &[("plank", 1), (THREAD, 2)],
            180,
            (FISHING_ROD, 1),
        ),
        recipe(
            COOK_FISH,
            "Cook fish",
            BAKE,
            &[(FISH, 2), (WATER, 1), ("log", 1)],
            200,
            (COOKED_MEAL, 2),
        ),
        recipe(
            WEAVE_COAT,
            "Weave warm coats",
            TAILOR,
            &[(THREAD, 8), (FIBER, 4)],
            480,
            (WARM_CLOTHES, 1),
        ),
    ]
}

fn profession(role: Role, station: &str, aptitudes: &[Role]) -> ProfessionDef {
    ProfessionDef {
        role,
        station: station.into(),
        outputs: Vec::new(),
        wants: Vec::new(),
        recipes: Vec::new(),
        tool: None,
        required_tool: None,
        keeps: Vec::new(),
        aptitudes: aptitudes.to_vec(),
        skill: None,
        template: "novice".into(),
        flexible_template: None,
        essential: false,
        endowment: Vec::new(),
        service: None,
        hunt: None,
        builds: false,
        sows: false,
        display: String::new(),
        tree: None,
    }
}

fn default_professions() -> Vec<ProfessionDef> {
    use Role::{
        Baker, Carpenter, Farmer, Fisher, Hunter, Innkeeper, Miller, Miner, Smith, Tailor, Tanner,
        WaterKeeper, Woodcutter,
    };
    let land = [Farmer, WaterKeeper, Woodcutter, Fisher];
    let grain = [Miller, Baker, Farmer];
    let metal = [Miner, Smith, Woodcutter];
    let hides = [Hunter, Tanner, Tailor, Farmer];
    let skill = |name: &str| {
        Some(SkillRequirement {
            name: name.into(),
            level: 8,
        })
    };
    vec![
        ProfessionDef {
            outputs: names(&[GRAIN, FIBER, BERRIES]),
            wants: names(&[HOE]),
            tool: Some(HOE.into()),
            essential: true,
            endowment: stacks(&[(GRAIN, 2), (FIBER, 2), (BERRIES, 2)]),
            sows: true,
            ..profession(Farmer, INN, &land)
        },
        ProfessionDef {
            outputs: names(&[FLOUR]),
            wants: names(&[GRAIN]),
            recipes: names(&[MILL]),
            // A buffer of grain: flour is milled the moment it is ordered.
            keeps: vec![KeepDef {
                good: GRAIN.into(),
                count: 4,
                reserve_inputs: true,
            }],
            essential: true,
            endowment: stacks(&[(FLOUR, 2)]),
            ..profession(Miller, MILL, &grain)
        },
        ProfessionDef {
            outputs: names(&["bread", COOKED_MEAL]),
            wants: names(&[FLOUR, "log", RAW_MEAT, FISH]),
            recipes: names(&[BAKE, COOK, COOK_FISH]),
            // A buffer of flour: bread is baked the moment it is ordered,
            // without first waiting on the miller and the farmer.
            keeps: vec![KeepDef {
                good: FLOUR.into(),
                count: 4,
                reserve_inputs: true,
            }],
            essential: true,
            endowment: stacks(&[(COOKED_MEAL, 2)]),
            ..profession(Baker, BAKE, &grain)
        },
        ProfessionDef {
            outputs: names(&[WATER]),
            essential: true,
            ..profession(WaterKeeper, INN, &land)
        },
        ProfessionDef {
            outputs: names(&["log"]),
            wants: names(&["axe"]),
            tool: Some("axe".into()),
            template: "woodcutter".into(),
            flexible_template: Some("carpenter".into()),
            essential: true,
            ..profession(Woodcutter, INN, &land)
        },
        ProfessionDef {
            outputs: names(&["plank", BOWL, FISHING_ROD]),
            wants: names(&["log", "plank", THREAD]),
            recipes: names(&[SAW, CARVE, MAKE_ROD]),
            skill: skill("Carpentry"),
            template: "carpenter".into(),
            endowment: stacks(&[("plank", 2), (BOWL, 2), (FISHING_ROD, 2)]),
            // The builder's tool: founded with one, keeps one (S2.2).
            tool: Some("hammer".into()),
            builds: true,
            ..profession(Carpenter, SAW, &[Carpenter, Woodcutter, Miller])
        },
        ProfessionDef {
            outputs: names(&["ore"]),
            wants: names(&["pickaxe"]),
            tool: Some("pickaxe".into()),
            template: "smith".into(),
            endowment: stacks(&[("ore", 2)]),
            ..profession(Miner, INN, &metal)
        },
        ProfessionDef {
            outputs: names(&["ingot", "hammer", SPEAR, "axe", "pickaxe", HOE]),
            wants: names(&["ore", "log", "ingot", "plank"]),
            recipes: names(&[
                SMELT,
                TOOL,
                FORGE_SPEAR,
                FORGE_AXE,
                FORGE_PICKAXE,
                FORGE_HOE,
            ]),
            skill: skill("Smithing"),
            template: "smith".into(),
            endowment: stacks(&[("ingot", 2), ("hammer", 2), (SPEAR, 2)]),
            ..profession(Smith, SMELT, &metal)
        },
        ProfessionDef {
            wants: names(&["log", BOWL]),
            keeps: [("log", 2), (BOWL, 1)]
                .into_iter()
                .map(|(good, count)| KeepDef {
                    good: good.into(),
                    count,
                    reserve_inputs: true,
                })
                .collect(),
            template: "smith".into(),
            service: Some(ServiceDef { wage_floor: 8 }),
            ..profession(Innkeeper, INN, &[Innkeeper, Baker, Miller, WaterKeeper])
        },
        ProfessionDef {
            outputs: names(&[RAW_MEAT, HIDE]),
            wants: names(&[SPEAR]),
            required_tool: Some(SPEAR.into()),
            endowment: stacks(&[(RAW_MEAT, 2), (HIDE, 2), (SPEAR, 2)]),
            hunt: Some(HuntDef {
                tool: SPEAR.into(),
                yields: stacks(&[(RAW_MEAT, 2), (HIDE, 1)]),
                prey: "deer".into(),
            }),
            ..profession(Hunter, INN, &hides)
        },
        ProfessionDef {
            outputs: names(&[LEATHER]),
            wants: names(&[HIDE, BOWL]),
            recipes: names(&[TAN]),
            endowment: stacks(&[(LEATHER, 2), (BOWL, 1)]),
            ..profession(Tanner, TAN, &hides)
        },
        ProfessionDef {
            outputs: names(&[THREAD, WARM_CLOTHES]),
            wants: names(&[LEATHER, FIBER, THREAD]),
            recipes: names(&[SPIN, TAILOR, WEAVE_COAT]),
            endowment: stacks(&[(THREAD, 2)]),
            // The last tailor of a village stays while somebody lacks a coat
            // (closing fix: in spring the coat trade emptied, and the chain
            // stood still when a resident wanted one).
            essential: true,
            ..profession(Tailor, TAILOR, &hides)
        },
        ProfessionDef {
            outputs: names(&[FISH]),
            wants: names(&[FISHING_ROD]),
            required_tool: Some(FISHING_ROD.into()),
            keeps: vec![KeepDef {
                good: FISHING_ROD.into(),
                count: 1,
                reserve_inputs: false,
            }],
            endowment: stacks(&[(FISH, 2), (FISHING_ROD, 1)]),
            ..profession(Fisher, INN, &land)
        },
    ]
}

fn patch(good: &str, dx: i32, dz: i32, capacity: u32, gather_ticks: u32) -> PatchDef {
    PatchDef {
        good: good.into(),
        dx,
        dz,
        at_site: None,
        biomes: Vec::new(),
        capacity,
        gather_ticks,
        finite: false,
        regrowth: 4,
        winter_regrowth: None,
        harvest_loss: 40,
        soil_loss: 0,
        shore: false,
    }
}

fn default_patches() -> Vec<PatchDef> {
    vec![
        // Wild grain is a founding reserve that does not come back: grain
        // grows on the fields residents sow.
        PatchDef {
            finite: true,
            regrowth: 0,
            harvest_loss: 0,
            ..patch(GRAIN, -30000, 10000, 120, 180)
        },
        PatchDef {
            harvest_loss: 125,
            ..patch("log", -32000, 0, 240, 240)
        },
        PatchDef {
            finite: true,
            regrowth: 0,
            harvest_loss: 0,
            ..patch("ore", 38000, 10000, 12000, 300)
        },
        PatchDef {
            at_site: Some(Role::WaterKeeper),
            regrowth: 10,
            harvest_loss: 0,
            ..patch(WATER, 0, 0, 160, 100)
        },
        patch(FIBER, -31000, 17000, 160, 120),
        patch(BERRIES, 38000, 0, 160, 80),
        PatchDef {
            regrowth: 2,
            shore: true,
            // 293 x 75% with a rod = 219: the catch rate of the rod-free
            // days before S2.2.
            ..patch(FISH, 38000, -8000, 80, 293)
        },
    ]
}

impl Default for EconomySpec {
    fn default() -> Self {
        let mut spec = Self {
            version: ECONOMY_VERSION,
            goods: default_goods(),
            recipes: default_recipes(),
            professions: default_professions(),
            patches: default_patches(),
            buildings: Vec::new(),
            seed: SeedSpec::default(),
            tuning: EconomyTuning::default(),
            demography: super::life_spec::DemographySpec::default(),
            crops: super::life_spec::default_crops(),
            weather: super::life_spec::WeatherSpec::default(),
            merchant: super::life_spec::MerchantSpec::default(),
            live: super::life_spec::LiveSpec::default(),
            props: super::props::default_props(),
        };
        spec.buildings = super::building_spec::default_buildings(&spec);
        spec
    }
}

fn duplicate<'a>(what: &str, keys: impl Iterator<Item = &'a str>) -> Result<(), String> {
    let mut seen = BTreeSet::new();
    for key in keys {
        if key.trim().is_empty() {
            return Err(format!("economy: a {what} has an empty name"));
        }
        if !seen.insert(key) {
            return Err(format!("economy: duplicate {what} `{key}`"));
        }
    }
    Ok(())
}

impl EconomySpec {
    pub fn good(&self, key: &str) -> Option<&GoodDef> {
        self.goods.iter().find(|good| good.key == key)
    }
    pub fn recipe(&self, key: &str) -> Option<&RecipeSpec> {
        self.recipes.iter().find(|recipe| recipe.key == key)
    }
    pub fn profession(&self, role: Role) -> Option<&ProfessionDef> {
        self.professions.iter().find(|entry| entry.role == role)
    }
    pub fn profession_mut(&mut self, role: Role) -> Option<&mut ProfessionDef> {
        self.professions.iter_mut().find(|entry| entry.role == role)
    }
    pub fn building(&self, id: &str) -> Option<&super::building_spec::BuildingDef> {
        self.buildings.iter().find(|def| def.id == id)
    }

    /// Every error names the offending entry.
    pub fn validate(&self) -> Result<(), String> {
        if self.version != ECONOMY_VERSION {
            return Err(format!(
                "economy: unsupported version {} (this build reads version {ECONOMY_VERSION})",
                self.version
            ));
        }
        self.validate_goods()?;
        self.validate_patches()?;
        self.validate_recipes()?;
        self.validate_professions()?;
        super::building_spec::validate(self)?;
        super::props::validate(self)?;
        self.validate_seed()?;
        self.validate_tuning()?;
        // Every role the living world's logic needs is filled by the data.
        crate::village::roles::from_spec(self)?;
        self.demography.validate()?;
        super::life_spec::validate_crops(self)?;
        self.weather.validate()?;
        super::life_spec::validate_merchant(self)?;
        super::life_spec::validate_live(self)
    }
    pub fn crop(&self, key: &str) -> Option<&super::life_spec::CropSpec> {
        self.crops.iter().find(|crop| crop.key == key)
    }

    fn known(&self, good: &str, owner: &str) -> Result<(), String> {
        if self.good(good).is_some() {
            Ok(())
        } else {
            Err(format!("economy: {owner} references unknown good `{good}`"))
        }
    }

    fn validate_goods(&self) -> Result<(), String> {
        if self.goods.is_empty() || self.goods.len() > 256 {
            return Err("economy: goods needs 1..=256 entries".into());
        }
        duplicate("good", self.goods.iter().map(|good| good.key.as_str()))?;
        for good in &self.goods {
            let name = &good.key;
            if good.display.trim().is_empty() {
                return Err(format!("economy: good `{name}` needs a display name"));
            }
            if !(1..=10_000).contains(&good.base_price) {
                return Err(format!(
                    "economy: good `{name}` base_price must be 1..=10000"
                ));
            }
            let food = good.tags.contains(&GoodTag::Food);
            match good.food_value {
                Some(value) if !food || !(1..=100_000).contains(&value) => {
                    return Err(format!(
                        "economy: good `{name}` food_value must be 1..=100000 and needs the food tag"
                    ));
                }
                None if food => {
                    return Err(format!(
                        "economy: good `{name}` has the food tag but no food_value"
                    ));
                }
                _ => {}
            }
            if good.durability == Some(0) {
                return Err(format!(
                    "economy: good `{name}` durability must be positive"
                ));
            }
            if let Some(tool) = &good.gather_tool {
                self.known(tool, &format!("good `{name}` gather_tool"))?;
            }
            match &good.item {
                Some(item) => {
                    if BUILTIN_ITEM_KINDS.contains(&name.as_str()) {
                        return Err(format!(
                            "economy: good `{name}` is an engine item kind and must not declare an item"
                        ));
                    }
                    if !(1..=1000).contains(&item.max_stack) || item.weight_g > 1_000_000 {
                        return Err(format!(
                            "economy: good `{name}` item needs max_stack 1..=1000 and weight_g up to 1000000"
                        ));
                    }
                }
                None if !BUILTIN_ITEM_KINDS.contains(&name.as_str()) => {
                    return Err(format!(
                        "economy: good `{name}` needs an item section (it is not an engine item kind)"
                    ));
                }
                None => {}
            }
        }
        Ok(())
    }

    fn validate_recipes(&self) -> Result<(), String> {
        duplicate(
            "recipe",
            self.recipes.iter().map(|recipe| recipe.key.as_str()),
        )?;
        for recipe in &self.recipes {
            let name = &recipe.key;
            let owner = format!("recipe `{name}`");
            if recipe.display.trim().is_empty() || recipe.station.trim().is_empty() {
                return Err(format!(
                    "economy: recipe `{name}` needs a display name and a station"
                ));
            }
            if !(1..=100_000).contains(&recipe.duration) {
                return Err(format!(
                    "economy: recipe `{name}` duration must be 1..=100000 ticks"
                ));
            }
            if recipe.outputs.is_empty() {
                return Err(format!("economy: recipe `{name}` has no output"));
            }
            for stack in recipe.inputs.iter().chain(&recipe.outputs) {
                self.known(&stack.good, &owner)?;
                if !(1..=1000).contains(&stack.count) {
                    return Err(format!(
                        "economy: recipe `{name}` quantity of `{}` must be 1..=1000",
                        stack.good
                    ));
                }
            }
            duplicate(
                &format!("input of recipe `{name}`"),
                recipe.inputs.iter().map(|stack| stack.good.as_str()),
            )?;
            if let Some(tool) = &recipe.tool {
                self.known(tool, &format!("recipe `{name}` tool"))?;
            }
            if !self
                .professions
                .iter()
                .any(|entry| entry.recipes.contains(name) && entry.station == recipe.station)
            {
                return Err(format!(
                    "economy: recipe `{name}` is not worked by any profession at station `{}`",
                    recipe.station
                ));
            }
        }
        // Every good must be reachable from a source: a patch, a profession that
        // obtains it without a recipe, or a recipe whose inputs are reachable.
        let mut reachable: BTreeSet<&str> = self
            .patches
            .iter()
            .map(|patch| patch.good.as_str())
            .collect();
        for entry in &self.professions {
            for output in &entry.outputs {
                if !self.recipes.iter().any(|recipe| {
                    entry.recipes.contains(&recipe.key)
                        && recipe.outputs.iter().any(|stack| &stack.good == output)
                }) {
                    reachable.insert(output);
                }
            }
        }
        loop {
            let before = reachable.len();
            for recipe in &self.recipes {
                if recipe
                    .inputs
                    .iter()
                    .all(|stack| reachable.contains(stack.good.as_str()))
                {
                    reachable.extend(recipe.outputs.iter().map(|stack| stack.good.as_str()));
                }
            }
            if reachable.len() == before {
                break;
            }
        }
        for recipe in &self.recipes {
            if let Some(stack) = recipe
                .inputs
                .iter()
                .find(|stack| !reachable.contains(stack.good.as_str()))
            {
                return Err(format!(
                    "economy: recipe `{}` is part of a cycle with no source: input `{}` can never be obtained",
                    recipe.key, stack.good
                ));
            }
        }
        // The funded plan orders goods by production depth, which needs an
        // acyclic good graph even when a source exists.
        let mut depth = BTreeMap::<&str, u32>::new();
        for _ in 0..=self.goods.len() {
            let mut changed = false;
            for recipe in &self.recipes {
                let inputs = recipe
                    .inputs
                    .iter()
                    .map(|stack| depth.get(stack.good.as_str()).copied().unwrap_or(0))
                    .max()
                    .unwrap_or(0);
                for stack in &recipe.outputs {
                    let entry = depth.entry(stack.good.as_str()).or_default();
                    if *entry < inputs + 1 {
                        *entry = inputs + 1;
                        changed = true;
                    }
                }
            }
            if !changed {
                return Ok(());
            }
        }
        let recipe = self
            .recipes
            .iter()
            .max_by_key(|recipe| {
                recipe
                    .outputs
                    .iter()
                    .map(|stack| depth.get(stack.good.as_str()).copied().unwrap_or(0))
                    .max()
                    .unwrap_or(0)
            })
            .map_or("", |recipe| recipe.key.as_str());
        Err(format!(
            "economy: recipe `{recipe}` is part of a recipe cycle (a good is its own ingredient)"
        ))
    }

    fn validate_professions(&self) -> Result<(), String> {
        // The catalog's professions are the trades: none is required by name
        // (the goods roles say which ones the world needs), none twice.
        if self.professions.is_empty() {
            return Err("economy: at least one profession is needed".into());
        }
        for role in self.professions.iter().map(|entry| entry.role) {
            let count = self
                .professions
                .iter()
                .filter(|entry| entry.role == role)
                .count();
            if count > 1 {
                return Err(format!(
                    "economy: duplicate profession `{}`",
                    role_key(role)
                ));
            }
        }
        for entry in &self.professions {
            let name = role_key(entry.role);
            let owner = format!("profession `{name}`");
            if entry.role == Role::Visitor {
                return Err("economy: `visitor` is not a resident profession".into());
            }
            if entry.station.trim().is_empty() || entry.template.trim().is_empty() {
                return Err(format!(
                    "economy: profession `{name}` needs a station and an actor template"
                ));
            }
            duplicate(
                &format!("output of profession `{name}`"),
                entry.outputs.iter().map(String::as_str),
            )?;
            for good in entry
                .outputs
                .iter()
                .chain(&entry.wants)
                .chain(&entry.tool)
                .chain(&entry.required_tool)
                .chain(entry.keeps.iter().map(|keep| &keep.good))
                .chain(entry.endowment.iter().map(|stack| &stack.good))
            {
                self.known(good, &owner)?;
            }
            for key in &entry.recipes {
                let Some(recipe) = self.recipe(key) else {
                    return Err(format!(
                        "economy: profession `{name}` references unknown recipe `{key}`"
                    ));
                };
                if recipe.station != entry.station {
                    return Err(format!(
                        "economy: profession `{name}` works at station `{}` but recipe `{key}` needs `{}`",
                        entry.station, recipe.station
                    ));
                }
                if !recipe
                    .outputs
                    .iter()
                    .any(|stack| entry.outputs.contains(&stack.good))
                {
                    return Err(format!(
                        "economy: profession `{name}` lists recipe `{key}` but none of its outputs"
                    ));
                }
                for stack in &recipe.inputs {
                    if !entry.outputs.contains(&stack.good)
                        && !entry.wants.contains(&stack.good)
                        && !self
                            .good(&stack.good)
                            .is_some_and(|good| good.tags.contains(&GoodTag::Staple))
                    {
                        return Err(format!(
                            "economy: profession `{name}` cannot buy `{}`, an input of its recipe `{key}`; add it to wants",
                            stack.good
                        ));
                    }
                }
            }
            for output in &entry.outputs {
                let crafted = entry.recipes.iter().any(|key| {
                    self.recipe(key)
                        .is_some_and(|recipe| recipe.outputs.iter().any(|s| &s.good == output))
                });
                let gathered = self.patches.iter().any(|patch| &patch.good == output);
                // Hunting is a physical activity outside the patch tables.
                let hunted = entry
                    .hunt
                    .as_ref()
                    .is_some_and(|hunt| hunt.yields.iter().any(|stack| &stack.good == output));
                if !crafted && !gathered && !hunted {
                    return Err(format!(
                        "economy: profession `{name}` has no producible output: `{output}` has neither a recipe of this profession nor a resource patch"
                    ));
                }
            }
            if entry.outputs.is_empty() && entry.keeps.is_empty() {
                return Err(format!(
                    "economy: profession `{name}` has no producible output and keeps no stock"
                ));
            }
            if !entry.aptitudes.contains(&entry.role) || entry.aptitudes.contains(&Role::Visitor) {
                return Err(format!(
                    "economy: profession `{name}` aptitudes must include itself and only resident professions"
                ));
            }
            if let Some(unknown) = entry
                .aptitudes
                .iter()
                .find(|aptitude| self.profession(**aptitude).is_none())
            {
                return Err(format!(
                    "economy: profession `{name}` has aptitude `{}`, which is not a profession",
                    role_key(*unknown)
                ));
            }
            for keep in &entry.keeps {
                if !(1..=100).contains(&keep.count) {
                    return Err(format!(
                        "economy: profession `{name}` keeps `{}` in a quantity outside 1..=100",
                        keep.good
                    ));
                }
            }
            if entry.endowment.iter().any(|stack| stack.count > 100) {
                return Err(format!(
                    "economy: profession `{name}` endowment quantities must be at most 100"
                ));
            }
        }
        Ok(())
    }

    fn validate_patches(&self) -> Result<(), String> {
        duplicate(
            "patch for good",
            self.patches.iter().map(|patch| patch.good.as_str()),
        )?;
        for (index, patch) in self.patches.iter().enumerate() {
            let owner = format!("patch {index} (`{}`)", patch.good);
            self.known(&patch.good, &owner)?;
            if !(1..=1_000_000).contains(&patch.capacity)
                || !(1..=100_000).contains(&patch.gather_ticks)
            {
                return Err(format!(
                    "economy: {owner} needs capacity 1..=1000000 and gather_ticks 1..=100000"
                ));
            }
            if patch.dx.abs() > 44_000 || patch.dz.abs() > 44_000 {
                return Err(format!(
                    "economy: {owner} must lie within 44 metres of its settlement centre"
                ));
            }
            if patch.finite && (patch.regrowth > 0 || patch.winter_regrowth.is_some()) {
                return Err(format!("economy: {owner} is finite and cannot regrow"));
            }
            if patch.regrowth > 1000
                || patch.winter_regrowth.is_some_and(|value| value > 1000)
                || patch.harvest_loss > 100_000
                || patch.soil_loss > 100_000
            {
                return Err(format!(
                    "economy: {owner} regrowth must be at most 1000 and losses at most 100000"
                ));
            }
            if patch.at_site == Some(Role::Visitor) {
                return Err(format!("economy: {owner} at_site must be a profession"));
            }
        }
        Ok(())
    }

    fn validate_seed(&self) -> Result<(), String> {
        let seed = &self.seed;
        if !(1..=1_000_000).contains(&seed.coins) {
            return Err("economy: seed.coins must be 1..=1000000".into());
        }
        if seed.founders.len() != FOUNDERS_PER_SETTLEMENT {
            return Err(format!(
                "economy: seed.founders needs exactly {FOUNDERS_PER_SETTLEMENT} entries"
            ));
        }
        if let Some(founder) = seed.founders.iter().find(|founder| {
            founder.role != Role::Visitor && self.profession(founder.role).is_none()
        }) {
            return Err(format!(
                "economy: seed.founders names unknown profession `{}`",
                role_key(founder.role)
            ));
        }
        if seed.founders.iter().any(|f| f.role == Role::Visitor) {
            return Err("economy: seed.founders must be resident professions".into());
        }
        for stack in &seed.common {
            self.known(&stack.good, "seed.common")?;
            if stack.count > 100 {
                return Err(format!(
                    "economy: seed.common quantity of `{}` must be at most 100",
                    stack.good
                ));
            }
        }
        if seed.food_preferences.is_empty() {
            return Err("economy: seed.food_preferences needs at least one food".into());
        }
        for food in &seed.food_preferences {
            self.known(food, "seed.food_preferences")?;
            if self.good(food).is_none_or(|good| good.food_value.is_none()) {
                return Err(format!(
                    "economy: seed.food_preferences entry `{food}` is not a food"
                ));
            }
        }
        if seed.endowment_bonus_max > 10 || seed.extra_aptitudes > 12 {
            return Err(
                "economy: seed bands need endowment_bonus_max <= 10 and extra_aptitudes <= 12"
                    .into(),
            );
        }
        Ok(())
    }

    fn validate_tuning(&self) -> Result<(), String> {
        let t = &self.tuning;
        if !(25..=400).contains(&t.build_tool_percent)
            || !(25..=400).contains(&t.build_bare_percent)
        {
            return Err(
                "economy: tuning.build_tool_percent and build_bare_percent must be 25..=400".into(),
            );
        }
        // Every work tool wears: it needs a durability (S2.2).
        let mut tools: Vec<&str> = Vec::new();
        for entry in &self.professions {
            tools.extend(entry.tool.as_deref());
            tools.extend(entry.required_tool.as_deref());
            tools.extend(entry.hunt.as_ref().map(|hunt| hunt.tool.as_str()));
        }
        tools.extend(
            self.goods
                .iter()
                .filter_map(|good| good.gather_tool.as_deref()),
        );
        for tool in tools {
            if self
                .good(tool)
                .is_some_and(|good| !good.durability.is_some_and(|d| (1..=100_000).contains(&d)))
            {
                return Err(format!(
                    "economy: `{tool}` is a work tool but has no `durability`"
                ));
            }
        }
        if !(100..=1000).contains(&t.storage_shelf_percent) {
            return Err("economy: tuning.storage_shelf_percent must be 100..=1000".into());
        }
        if let Some(good) = self.goods.iter().find(|good| {
            good.shelf_life_days
                .is_some_and(|days| !(1..=1000).contains(&days))
        }) {
            return Err(format!(
                "economy: good `{}` shelf_life_days must be 1..=1000",
                good.key
            ));
        }
        if let Some(good) = self
            .goods
            .iter()
            .find(|good| good.gather_tool_required && good.gather_tool.is_none())
        {
            return Err(format!(
                "economy: good `{}` needs a `gather_tool` to require one",
                good.key
            ));
        }
        if !(1..=1000).contains(&t.inn_bed_price) || !(1..=24_000).contains(&t.inn_bed_ticks) {
            return Err(
                "economy: tuning.inn_bed_price must be 1..=1000 and inn_bed_ticks 1..=24000".into(),
            );
        }
        let checks: [(bool, &str); 18] = [
            (t.levy_percent <= 100, "levy_percent must be 0..=100"),
            (
                t.destitution_floor <= t.levy_threshold,
                "destitution_floor must not exceed levy_threshold",
            ),
            (
                (0..=1000).contains(&t.margin_gap_permille),
                "margin_gap_permille must be 0..=1000",
            ),
            (
                (1..=100).contains(&t.tool_gather_percent)
                    && (100..=1000).contains(&t.bare_gather_percent),
                "tool_gather_percent must be 1..=100 and bare_gather_percent 100..=1000",
            ),
            (
                (1..=t.work_pace_slowest_percent).contains(&t.work_pace_fastest_percent)
                    && t.work_pace_slowest_percent <= 400,
                "work pace needs 1 <= fastest <= slowest <= 400",
            ),
            (
                (0..=100_000).contains(&t.cold_health_threshold)
                    && (0..=100_000).contains(&t.cold_health_drain),
                "cold_health_threshold and cold_health_drain must be 0..=100000",
            ),
            (
                (1..=24).contains(&t.winter_hearth_lightings),
                "winter_hearth_lightings must be 1..=24",
            ),
            (
                (100..=100_000).contains(&t.price_ceiling_percent),
                "price_ceiling_percent must be 100..=100000",
            ),
            (
                t.winter_fuel_lead_days <= 7 && t.winter_fuel_reserve <= 20,
                "winter_fuel_lead_days must be at most 7 and winter_fuel_reserve at most 20",
            ),
            (
                t.vacated_seat_ticks <= t.profession_return_ticks
                    && t.profession_return_ticks <= 672_000,
                "vacated_seat_ticks must not exceed profession_return_ticks (at most 672000)",
            ),
            (
                t.depot_working_stock <= 100
                    && (1..=100).contains(&t.depot_deposit_min)
                    && (1..=100).contains(&t.stall_restock_level),
                "depot_working_stock must be at most 100; depot_deposit_min and stall_restock_level 1..=100",
            ),
            (
                (1..=20).contains(&t.haul_trip_units)
                    && (100..=24_000).contains(&t.haul_reservation_ticks)
                    && (20..=10_000).contains(&t.build_stage_ticks),
                "haul_trip_units must be 1..=20, haul_reservation_ticks 100..=24000 and build_stage_ticks 20..=10000",
            ),
            (
                t.civic_reserve <= 1_000_000 && t.civic_budget_slack_percent <= 400,
                "civic_reserve must be at most 1000000 and civic_budget_slack_percent at most 400",
            ),
            (
                (100..=48_000).contains(&t.planner_review_ticks)
                    && (1..=100).contains(&t.planner_sustain_reviews)
                    && (4..=64).contains(&t.planner_search_radius_cells),
                "planner_review_ticks must be 100..=48000, planner_sustain_reviews 1..=100 and planner_search_radius_cells 4..=64",
            ),
            (
                (1..=100_000).contains(&t.planner_storage_overflow_units)
                    && (1..=100_000).contains(&t.planner_demand_units)
                    && (2_400..=2_400_000).contains(&t.planner_project_timeout_ticks),
                "planner_storage_overflow_units and planner_demand_units must be 1..=100000; planner_project_timeout_ticks 2400..=2400000",
            ),
            (
                (1..=28).contains(&t.days_per_season)
                    && t.civic_reserve_grant_days <= 28
                    && (1..=28).contains(&t.civic_reserve_window_days)
                    && (1..=100_000).contains(&t.planner_crop_shortage_units)
                    && (-1..=100_000).contains(&t.founding_day)
                    && t.building_levy_percent <= 100
                    && t.relief_levy_percent <= 100
                    && (1..=1000).contains(&t.sleep_energy_per_tick)
                    && (1..=1000).contains(&t.nap_energy_per_tick)
                    && t.gatherer_shelf_units <= 100
                    && t.price_floor_percent <= 100
                    && (1..=64).contains(&t.inn_guest_beds),
                "days_per_season and civic_reserve_window_days must be 1..=28, civic_reserve_grant_days at most 28, planner_crop_shortage_units 1..=100000, founding_day -1..=100000, building_levy_percent and relief_levy_percent at most 100 and sleep_energy_per_tick and nap_energy_per_tick 1..=1000, gatherer_shelf_units and price_floor_percent at most 100, inn_guest_beds 1..=64",
            ),
            (
                t.maker_margin_percent <= 1000
                    && t.raw_floor_percent <= 100
                    && t.stock_target_days <= 28
                    && t.staple_stock_days <= 28
                    && t.staple_winter_stock_days <= 28
                    && t.staple_advance_per_maker <= 10_000
                    && t.staple_advance_per_day <= 100_000
                    && (1..=256).contains(&t.trade_window_rounds)
                    && t.boat_duty_percent <= 100,
                "maker_margin_percent must be at most 1000, raw_floor_percent and boat_duty_percent at most 100, stock_target_days, staple_stock_days and staple_winter_stock_days at most 28, staple_advance_per_maker at most 10000, staple_advance_per_day at most 100000 and trade_window_rounds 1..=256",
            ),
            (
                (0..=100_000).contains(&t.freezing_warmth)
                    && (0..=100_000).contains(&t.exposure_warmth)
                    && t.coat_lead_days <= 28,
                "freezing_warmth and exposure_warmth must be 0..=100000 and coat_lead_days at most 28",
            ),
        ];
        for (ok, message) in checks {
            if !ok {
                return Err(format!("economy: tuning.{message}"));
            }
        }
        Ok(())
    }

    pub fn sim_hash(&self, h: &mut Fnv64) {
        let text = |h: &mut Fnv64, value: &Option<String>| h.opt(value.as_ref(), |h, v| h.str(v));
        let list = |h: &mut Fnv64, values: &[String]| {
            h.u32(values.len() as u32);
            for value in values {
                h.str(value);
            }
        };
        let pile = |h: &mut Fnv64, values: &[Stack]| {
            h.u32(values.len() as u32);
            for value in values {
                h.str(&value.good);
                h.u32(value.count);
            }
        };
        let skill = |h: &mut Fnv64, value: &Option<SkillRequirement>| {
            h.opt(value.as_ref(), |h, v| {
                h.str(&v.name);
                h.i32(v.level);
            });
        };
        h.u32(self.version);
        h.u32(self.goods.len() as u32);
        for good in &self.goods {
            h.str(&good.key);
            h.str(&good.display);
            h.u32(good.base_price);
            h.opt(good.food_value.as_ref(), |h, v| h.i32(*v));
            h.u32(good.tags.len() as u32);
            for tag in &good.tags {
                h.u8(*tag as u8);
            }
            h.opt(good.durability.as_ref(), |h, v| h.u32(*v));
            text(h, &good.gather_tool);
            h.u8(u8::from(good.gather_tool_required));
            h.opt(good.shelf_life_days.as_ref(), |h, v| h.u32(*v));
            h.opt(good.item.as_ref(), |h, item| {
                h.u32(item.weight_g);
                h.u32(item.max_stack);
                list(h, &item.tags);
                text(h, &item.equip_slot);
            });
        }
        h.u32(self.recipes.len() as u32);
        for recipe in &self.recipes {
            h.str(&recipe.key);
            h.str(&recipe.display);
            h.str(&recipe.station);
            pile(h, &recipe.inputs);
            pile(h, &recipe.outputs);
            h.u32(recipe.duration);
            text(h, &recipe.tool);
            skill(h, &recipe.skill);
        }
        h.u32(self.professions.len() as u32);
        for entry in &self.professions {
            entry.role.sim_hash(h);
            h.str(&entry.station);
            list(h, &entry.outputs);
            list(h, &entry.wants);
            list(h, &entry.recipes);
            text(h, &entry.tool);
            text(h, &entry.required_tool);
            h.u32(entry.keeps.len() as u32);
            for keep in &entry.keeps {
                h.str(&keep.good);
                h.u32(keep.count);
                h.u8(u8::from(keep.reserve_inputs));
            }
            h.u32(entry.aptitudes.len() as u32);
            for role in &entry.aptitudes {
                role.sim_hash(h);
            }
            skill(h, &entry.skill);
            h.str(&entry.template);
            text(h, &entry.flexible_template);
            h.u8(u8::from(entry.essential));
            pile(h, &entry.endowment);
            h.opt(entry.service.as_ref(), |h, service| {
                h.u32(service.wage_floor)
            });
            h.opt(entry.hunt.as_ref(), |h, hunt| {
                h.str(&hunt.tool);
                pile(h, &hunt.yields);
                h.str(&hunt.prey);
            });
            h.u8(u8::from(entry.builds));
            h.u8(u8::from(entry.sows));
            h.str(&entry.display);
            h.opt(entry.tree.as_ref(), |h, tree| h.str(tree));
        }
        h.u32(self.patches.len() as u32);
        for patch in &self.patches {
            h.str(&patch.good);
            h.i32(patch.dx);
            h.i32(patch.dz);
            h.opt(patch.at_site.as_ref(), |h, role| role.sim_hash(h));
            h.u32(patch.biomes.len() as u32);
            for biome in &patch.biomes {
                h.u8(*biome as u8);
            }
            h.u32(patch.capacity);
            h.u32(patch.gather_ticks);
            h.u8(u8::from(patch.finite));
            h.u32(patch.regrowth);
            h.opt(patch.winter_regrowth.as_ref(), |h, v| h.u32(*v));
            h.u32(patch.harvest_loss);
            h.u32(patch.soil_loss);
            h.u8(u8::from(patch.shore));
        }
        let seed = &self.seed;
        h.u32(seed.coins);
        pile(h, &seed.common);
        h.u32(seed.founders.len() as u32);
        for founder in &seed.founders {
            founder.role.sim_hash(h);
            h.u8(u8::from(founder.flexible));
        }
        list(h, &seed.food_preferences);
        h.u32(seed.endowment_bonus_max);
        h.u32(seed.extra_aptitudes);
        let t = &self.tuning;
        for value in [
            t.levy_threshold,
            t.levy_percent,
            t.destitution_floor,
            t.fund_reserve_per_resident,
            t.tool_gather_percent,
            t.bare_gather_percent,
            t.build_tool_percent,
            t.build_bare_percent,
            t.storage_shelf_percent,
            t.work_pace_slowest_percent,
            t.work_pace_fastest_percent,
            t.winter_hearth_lightings,
            t.price_ceiling_percent,
            t.winter_fuel_reserve,
            t.depot_working_stock,
            t.depot_deposit_min,
            t.stall_restock_level,
            t.haul_trip_units,
            t.build_stage_ticks,
            t.civic_reserve,
            t.civic_budget_slack_percent,
            t.planner_sustain_reviews,
            t.planner_storage_overflow_units,
            t.planner_demand_units,
            t.planner_search_radius_cells,
        ] {
            h.u32(value);
        }
        for value in [
            t.margin_persist_ticks,
            t.margin_cooldown_ticks,
            t.switch_cooldown_ticks,
            t.winter_fuel_lead_days,
            t.profession_return_ticks,
            t.vacated_seat_ticks,
            t.haul_reservation_ticks,
            t.planner_review_ticks,
            t.planner_project_timeout_ticks,
        ] {
            h.u64(value);
        }
        h.u64(t.margin_gap_permille as u64);
        h.i32(t.cold_health_threshold);
        h.i32(t.cold_health_drain);
        super::building_spec::sim_hash(&self.buildings, h);
        super::props::sim_hash(&self.props, h);
        h.u64(t.days_per_season);
        h.u32(t.civic_reserve_grant_days);
        h.u32(t.civic_reserve_window_days);
        h.u32(t.planner_crop_shortage_units);
        h.u64(t.founding_day as u64);
        h.u32(t.building_levy_percent);
        h.u32(t.relief_levy_percent);
        h.u32(t.sleep_energy_per_tick);
        h.u32(t.nap_energy_per_tick);
        h.u32(t.gatherer_shelf_units);
        h.u32(t.price_floor_percent);
        h.u32(t.inn_guest_beds);
        h.u8(u8::from(t.reservation_prices));
        h.u32(t.maker_margin_percent);
        h.u32(t.raw_floor_percent);
        h.u32(t.stock_target_days);
        h.u32(t.staple_stock_days);
        h.u32(t.staple_winter_stock_days);
        h.u32(t.staple_advance_per_day);
        h.u32(t.staple_advance_per_maker);
        h.u8(u8::from(t.stock_cover_pricing));
        h.u32(t.trade_window_rounds);
        h.u32(t.boat_duty_percent);
        h.i32(t.freezing_warmth);
        h.i32(t.exposure_warmth);
        h.u32(t.coat_lead_days);
        h.u8(u8::from(t.raw_stock_cover));
        h.u32(t.idle_trade_days);
        h.u32(t.inn_detour_m);
        h.u8(u8::from(t.unused_goods_to_market));
        h.u32(t.coat_shelf_residents);
        h.u8(u8::from(t.coat_grants_any_season));
        h.u8(u8::from(t.grants_spare_sites));
        h.u32(t.founder_coats_per_three);
        h.u8(u8::from(t.wildlife_motion));
        h.u8(u8::from(t.found_coat_workshops));
        h.u32(t.inn_bed_price);
        h.u64(t.inn_bed_ticks);
        self.demography.sim_hash(h);
        super::life_spec::hash_crops(&self.crops, h);
        self.weather.sim_hash(h);
        self.merchant.sim_hash(h);
        self.live.sim_hash(h);
    }
}

/// The JSON name of a profession: its data key.
pub fn role_key(role: Role) -> &'static str {
    role.key()
}

/// Every trade of the world's catalog, in role order; the fixed shipped
/// table outside the living world.
pub fn trades(world: &World) -> Vec<Role> {
    registry(world).map_or_else(
        || ROLES.to_vec(),
        |registry| {
            let mut out: Vec<Role> = registry.spec.professions.iter().map(|p| p.role).collect();
            out.sort_unstable();
            out
        },
    )
}

/// A service trade (one keeper per workplace, paid by guests): the
/// profession's `service` entry in the living world, the innkeeper elsewhere.
pub fn is_service(world: &World, role: Role) -> bool {
    registry(world).map_or(role == Role::Innkeeper, |registry| {
        registry
            .definition(role)
            .is_some_and(|definition| definition.service.is_some())
    })
}

/// A trade as players read it: the catalog's `display`, else the role's own
/// name (`well keeper`; a data trade's key).
pub fn trade_label(world: &World, role: Role) -> &'static str {
    registry(world)
        .and_then(|registry| registry.profession(role))
        .map_or_else(|| role.name(), |entry| entry.display)
}

/// Whether a profession's workplace is an open yard: from the catalog in the
/// living world, the well and the fishing spot elsewhere.
pub fn open_site(world: &World, role: Role) -> bool {
    registry(world).map_or(
        matches!(role, Role::WaterKeeper | Role::Fisher),
        |registry| open_workplace(&registry.spec, role),
    )
}

/// Whether the profession's workplace has an open yard instead of a solid
/// body (a well, a fishing hut): the catalog workplace building's `solid`.
pub fn open_workplace(spec: &EconomySpec, role: Role) -> bool {
    spec.buildings.iter().any(|building| {
        !building.solid
            && matches!(&building.kind, super::building_spec::BuildingKind::Workplace { profession, .. } if *profession == role)
    })
}

// Catalog accessors hand out `&'static` names, as the Rust tables always did.
// Authored names are interned once per distinct value, so repeated worlds in
// one process share them. Only values are compared, never addresses.
pub fn intern(text: &str) -> &'static str {
    static POOL: OnceLock<Mutex<BTreeSet<&'static str>>> = OnceLock::new();
    let mut pool = POOL
        .get_or_init(Mutex::default)
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    if let Some(found) = pool.get(text) {
        return found;
    }
    let leaked: &'static str = Box::leak(text.to_owned().into_boxed_str());
    pool.insert(leaked);
    leaked
}
fn intern_list(items: Vec<&'static str>) -> &'static [&'static str] {
    static POOL: OnceLock<Mutex<BTreeSet<&'static [&'static str]>>> = OnceLock::new();
    let mut pool = POOL
        .get_or_init(Mutex::default)
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    if let Some(found) = pool.get(items.as_slice()) {
        return found;
    }
    let leaked: &'static [&'static str] = Box::leak(items.into_boxed_slice());
    pool.insert(leaked);
    leaked
}
fn intern_all(values: &[String]) -> &'static [&'static str] {
    intern_list(values.iter().map(|value| intern(value)).collect())
}

#[derive(Clone, Debug)]
pub struct GoodEntry {
    pub key: &'static str,
    pub display: &'static str,
    pub base_price: u32,
    pub food_value: Option<i32>,
    pub staple: bool,
    pub durability: Option<u32>,
    pub gather_tool: Option<&'static str>,
    pub gather_tool_required: bool,
}
#[derive(Clone, Debug)]
pub struct ProfessionEntry {
    pub outputs: &'static [&'static str],
    /// Authored wants, the tool and every staple.
    pub wants: &'static [&'static str],
    pub recipes: &'static [&'static str],
    pub tool: Option<&'static str>,
    pub required_tool: Option<&'static str>,
    pub station: &'static str,
    pub essential: bool,
    /// The trade as players read it: `display`, else the role's own name.
    pub display: &'static str,
}

/// Frozen lookups derived from the world's [`EconomySpec`]. Not hashed itself:
/// the spec it mirrors is hashed with the world construction.
#[derive(Resource, Clone, Debug)]
pub struct EconomyRegistry {
    pub spec: EconomySpec,
    goods: BTreeMap<&'static str, GoodEntry>,
    professions: BTreeMap<Role, ProfessionEntry>,
    recipe_labels: BTreeMap<&'static str, &'static str>,
    trade_kinds: &'static [&'static str],
    foods: &'static [&'static str],
    food_chain: BTreeSet<&'static str>,
}
impl EconomyRegistry {
    pub fn new(spec: &EconomySpec) -> Self {
        let goods: BTreeMap<_, _> = spec
            .goods
            .iter()
            .map(|good| {
                let key = intern(&good.key);
                (
                    key,
                    GoodEntry {
                        key,
                        display: intern(&good.display),
                        base_price: good.base_price,
                        food_value: good.food_value,
                        staple: good.tags.contains(&GoodTag::Staple),
                        durability: good.durability,
                        gather_tool: good.gather_tool.as_deref().map(intern),
                        gather_tool_required: good.gather_tool_required,
                    },
                )
            })
            .collect();
        let professions = spec
            .professions
            .iter()
            .map(|entry| {
                let mut wants: BTreeSet<&'static str> =
                    entry.wants.iter().map(|good| intern(good)).collect();
                wants.extend(entry.tool.as_deref().map(intern));
                wants.extend(goods.values().filter(|g| g.staple).map(|g| g.key));
                (
                    entry.role,
                    ProfessionEntry {
                        outputs: intern_all(&entry.outputs),
                        wants: intern_list(wants.into_iter().collect()),
                        recipes: intern_all(&entry.recipes),
                        tool: entry.tool.as_deref().map(intern),
                        required_tool: entry.required_tool.as_deref().map(intern),
                        station: intern(&entry.station),
                        essential: entry.essential,
                        display: if entry.display.trim().is_empty() {
                            entry.role.name()
                        } else {
                            intern(&entry.display)
                        },
                    },
                )
            })
            .collect();
        // Food, water and everything a recipe turns into them, to any depth.
        let mut food_chain: BTreeSet<&'static str> = goods
            .values()
            .filter(|good| good.food_value.is_some() || good.key == WATER)
            .map(|good| good.key)
            .collect();
        loop {
            let before = food_chain.len();
            for recipe in &spec.recipes {
                if recipe
                    .outputs
                    .iter()
                    .any(|output| food_chain.contains(output.good.as_str()))
                {
                    food_chain.extend(recipe.inputs.iter().map(|input| intern(&input.good)));
                }
            }
            if food_chain.len() == before {
                break;
            }
        }
        Self {
            food_chain,
            trade_kinds: intern_list(goods.keys().copied().collect()),
            foods: intern_list(
                goods
                    .values()
                    .filter(|good| good.food_value.is_some())
                    .map(|good| good.key)
                    .collect(),
            ),
            recipe_labels: spec
                .recipes
                .iter()
                .map(|recipe| (intern(&recipe.key), intern(&recipe.display)))
                .collect(),
            goods,
            professions,
            spec: spec.clone(),
        }
    }
    pub fn good(&self, key: &str) -> Option<&GoodEntry> {
        self.goods.get(key)
    }
    pub fn profession(&self, role: Role) -> Option<&ProfessionEntry> {
        self.professions.get(&role)
    }
    pub fn definition(&self, role: Role) -> Option<&ProfessionDef> {
        self.spec.profession(role)
    }
    /// Every tradable good, sorted by stable key.
    pub fn trade_kinds(&self) -> &'static [&'static str] {
        self.trade_kinds
    }
    /// Every edible good, sorted by stable key.
    pub fn foods(&self) -> &'static [&'static str] {
        self.foods
    }
    /// Whether the good is food or water, or an ingredient of one at any depth
    /// (grain and flour for bread, raw meat and fish for meals, their fuel).
    pub fn feeds_food(&self, key: &str) -> bool {
        self.food_chain.contains(key)
    }
    pub fn recipe_label(&self, key: &str) -> Option<&'static str> {
        self.recipe_labels.get(key).copied()
    }
    pub fn tuning(&self) -> &EconomyTuning {
        &self.spec.tuning
    }
    /// The interned name of a known good.
    pub fn name(&self, key: &str) -> Option<&'static str> {
        self.goods.get(key).map(|good| good.key)
    }
}

pub fn registry(world: &World) -> Option<&EconomyRegistry> {
    world.get_resource::<EconomyRegistry>()
}
/// Regional economy tuning; `None` outside the regional profile.
pub fn tuning(world: &World) -> Option<&EconomyTuning> {
    registry(world).map(EconomyRegistry::tuning)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fails(edit: impl FnOnce(&mut EconomySpec), expected: &str) {
        let mut spec = EconomySpec::default();
        edit(&mut spec);
        let error = spec.validate().expect_err(expected);
        assert!(
            error.contains(expected),
            "expected `{expected}` in `{error}`"
        );
    }

    #[test]
    fn default_economy_is_valid_and_round_trips_through_json() {
        let spec = EconomySpec::default();
        spec.validate().unwrap();
        let text = serde_json::to_string(&spec).unwrap();
        assert_eq!(serde_json::from_str::<EconomySpec>(&text).unwrap(), spec);
        assert_eq!(serde_json::from_str::<EconomySpec>("{}").unwrap(), spec);
        assert!(serde_json::from_str::<EconomySpec>(r#"{"good":[]}"#).is_err());
    }

    #[test]
    fn unsupported_version_is_rejected() {
        fails(|spec| spec.version = 2, "unsupported version 2");
    }

    #[test]
    fn duplicate_names_are_rejected_by_name() {
        fails(
            |spec| spec.goods.push(spec.goods[0].clone()),
            "duplicate good `axe`",
        );
        fails(
            |spec| spec.recipes.push(spec.recipes[0].clone()),
            "duplicate recipe `village_mill`",
        );
        fails(
            |spec| spec.professions.push(spec.professions[0].clone()),
            "duplicate profession `farmer`",
        );
    }

    #[test]
    fn a_recipe_referencing_an_unknown_good_is_rejected_by_name() {
        fails(
            |spec| spec.recipes[0].inputs[0].good = "village_rye".into(),
            "recipe `village_mill` references unknown good `village_rye`",
        );
        fails(
            |spec| spec.recipes[0].outputs[0].good = "village_meal".into(),
            "recipe `village_mill` references unknown good `village_meal`",
        );
    }

    #[test]
    fn a_recipe_cycle_with_no_source_is_rejected_by_name() {
        // Flour now needs bread and bread needs flour: neither has a source.
        fails(
            |spec| {
                spec.recipes[0].inputs = stacks(&[("bread", 1)]);
                spec.profession_mut(Role::Miller)
                    .unwrap()
                    .wants
                    .push("bread".into());
            },
            "recipe `village_mill` is part of a cycle with no source: input `bread`",
        );
        // A sourced good that is also its own ingredient still has no depth.
        fails(
            |spec| {
                spec.recipes[2].inputs.push(Stack {
                    good: "plank".into(),
                    count: 1,
                });
            },
            "recipe `village_saw` is part of a cycle with no source: input `plank`",
        );
        // Sourced goods that are each other's ingredient still have no depth.
        fails(
            |spec| {
                spec.recipes.push(recipe(
                    "village_split",
                    "Split bowls",
                    SAW,
                    &[(BOWL, 1)],
                    100,
                    ("plank", 1),
                ));
                spec.profession_mut(Role::Carpenter)
                    .unwrap()
                    .recipes
                    .push("village_split".into());
            },
            "is part of a recipe cycle",
        );
    }

    #[test]
    fn a_profession_with_no_producible_output_is_rejected_by_name() {
        fails(
            |spec| {
                spec.profession_mut(Role::Miner)
                    .unwrap()
                    .outputs
                    .push("ingot".into());
            },
            "profession `miner` has no producible output: `ingot`",
        );
        fails(
            |spec| {
                let inn = spec.profession_mut(Role::Innkeeper).unwrap();
                inn.keeps.clear();
            },
            "profession `innkeeper` has no producible output and keeps no stock",
        );
    }

    #[test]
    fn zero_and_out_of_range_durations_are_rejected_by_name() {
        fails(
            |spec| spec.recipes[1].duration = 0,
            "recipe `village_bake` duration must be 1..=100000",
        );
        fails(
            |spec| spec.patches[0].gather_ticks = 0,
            "patch 0 (`village_grain`) needs capacity",
        );
        // Durations are unsigned, so a negative number fails while parsing.
        let error = serde_json::from_str::<EconomySpec>(
            r#"{"recipes":[{"key":"x","display":"X","station":"s","inputs":[],
                "outputs":[],"duration":-5}]}"#,
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("-5"), "{error}");
    }

    #[test]
    fn other_references_and_ranges_are_checked() {
        fails(
            |spec| {
                spec.profession_mut(Role::Baker)
                    .unwrap()
                    .recipes
                    .push("village_fry".into());
            },
            "profession `baker` references unknown recipe `village_fry`",
        );
        fails(
            |spec| spec.profession_mut(Role::Baker).unwrap().wants.clear(),
            "profession `baker` cannot buy `village_flour`",
        );
        fails(
            |spec| spec.patches[1].good = "village_peat".into(),
            "patch 1 (`village_peat`) references unknown good `village_peat`",
        );
        fails(
            |spec| spec.goods[1].food_value = None,
            "good `bread` has the food tag but no food_value",
        );
        fails(
            |spec| spec.goods[0].base_price = 0,
            "good `axe` base_price must be 1..=10000",
        );
        fails(
            |spec| spec.seed.founders.truncate(13),
            "seed.founders needs exactly 14 entries",
        );
        fails(
            |spec| spec.tuning.levy_percent = 101,
            "tuning.levy_percent must be 0..=100",
        );
        fails(
            |spec| spec.professions.retain(|entry| entry.role != Role::Fisher),
            // A trade others still name is refused where it is named.
            "`fisher`",
        );
    }

    /// The shipped data and the default block hold together on their own
    /// (the Rust tables they replaced are gone): every tool, recipe good,
    /// patch good, profession good and recipe resolves to a good of the
    /// block, every role the code reads is filled by a tag, and fish needs
    /// the rod.
    #[test]
    fn shipped_economy_data_resolves_every_reference() {
        let shipped = crate::worlds::builtin_preset("living_world")
            .and_then(|preset| preset.living_world)
            .expect("the shipped world")
            .economy;
        for spec in [EconomySpec::default(), shipped] {
            spec.validate().expect("valid");
            let goods: BTreeSet<&str> = spec.goods.iter().map(|good| good.key.as_str()).collect();
            let recipes: BTreeSet<&str> = spec
                .recipes
                .iter()
                .map(|recipe| recipe.key.as_str())
                .collect();
            let known = |what: &str, key: &str| {
                assert!(goods.contains(key), "{what} `{key}` is not a good");
            };
            for good in &spec.goods {
                if let Some(tool) = &good.gather_tool {
                    known("gather tool", tool);
                }
                assert!(
                    good.item.is_some() || BUILTIN_ITEM_KINDS.contains(&good.key.as_str()),
                    "good `{}` has no item and is not built in",
                    good.key
                );
            }
            for recipe in &spec.recipes {
                for stack in recipe.inputs.iter().chain(&recipe.outputs) {
                    known("recipe good", &stack.good);
                }
                if let Some(tool) = &recipe.tool {
                    known("recipe tool", tool);
                }
            }
            for patch in &spec.patches {
                known("patch good", &patch.good);
            }
            for profession in &spec.professions {
                for key in profession.outputs.iter().chain(&profession.wants) {
                    known("profession good", key);
                }
                for tool in profession.tool.iter().chain(&profession.required_tool) {
                    known("profession tool", tool);
                }
                for recipe in &profession.recipes {
                    assert!(recipes.contains(recipe.as_str()), "recipe `{recipe}`");
                }
            }
            crate::village::roles::from_spec(&spec).expect("every role filled by a tag");
            let fish = spec
                .goods
                .iter()
                .find(|good| good.key == FISH)
                .expect("fish");
            assert_eq!(fish.gather_tool.as_deref(), Some(FISHING_ROD));
            assert!(fish.gather_tool_required, "fish needs the rod");
        }
    }
}
