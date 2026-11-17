//! Demography, crops, weather and the merchant boat as data: four sections of
//! the regional `economy` block. Each is optional; a missing section, or a
//! missing field inside one, takes the built-in value. Validation and hashing
//! are driven from [`super::economy_spec::EconomySpec`].
use super::economy_spec::{EconomySpec, Stack};
use crate::village::Season;
use bevy_reflect::Reflect;
use npc_framework::prelude::*;
use serde::{Deserialize, Serialize};

fn season_tag(season: Season) -> u8 {
    match season {
        Season::Spring => 0,
        Season::Summer => 1,
        Season::Autumn => 2,
        Season::Winter => 3,
    }
}

/// Ages are counted in seasons, so a shorter year (`tuning.days_per_season`)
/// shortens every life by the same factor.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct DemographySpec {
    pub enabled: bool,
    /// A child comes of age at this age.
    pub adult_age_seasons: u32,
    /// An adult becomes an elder at this age.
    pub elder_age_seasons: u32,
    /// Old age takes a resident at a seeded age in this band.
    pub lifespan_min_seasons: u32,
    pub lifespan_max_seasons: u32,
    /// Founders and arrivals are between `adult_age_seasons` and this age.
    pub founder_max_age_seasons: u32,
    /// Mutual affinity two housed adults need before they court.
    pub pair_affinity: i32,
    /// Days the affinity must last before the two become a couple.
    pub courtship_days: u32,
    /// Days a couple waits after a birth before the next.
    pub birth_cooldown_days: u32,
    /// Daily chance of a birth for a couple that may have a child.
    pub birth_chance_permille: u32,
    /// Food units the household can count on before a birth (its members' packs
    /// plus its share of its settlement's stored food).
    pub birth_food_units: u32,
    /// Most children one household raises at a time.
    pub max_children: u32,
    /// Percent of each settlement's founding adults who arrive as couples,
    /// sharing a home (closing fix B: with none, the first couple formed only
    /// in mid-year).
    pub founding_couples_percent: u32,
    /// A child's hunger as a share of an adult's.
    pub child_ration_percent: u32,
    /// Days a child may go unfed before it dies.
    pub child_starve_days: u32,
    /// Walking speed and work duration of an elder, percent of an adult's.
    pub elder_speed_percent: u32,
    pub elder_work_percent: u32,
    /// Most settlers one boat brings.
    pub arrivals_per_visit: u32,
    /// Coins the civic fund gives a settler on arrival, while it can pay.
    pub arrival_grant_coins: u32,
    /// Food units per head a settlement must hold before settlers land there.
    pub arrival_food_per_head: u32,
    /// Settlers stop landing above this many living residents and children.
    pub max_population: u32,
    /// Given names of children and settlers, drawn with the seed.
    pub names: Vec<String>,
}

impl Default for DemographySpec {
    fn default() -> Self {
        Self {
            enabled: true,
            adult_age_seasons: 8,
            elder_age_seasons: 36,
            lifespan_min_seasons: 40,
            lifespan_max_seasons: 52,
            founder_max_age_seasons: 34,
            pair_affinity: 12_000,
            courtship_days: 2,
            birth_cooldown_days: 10,
            birth_chance_permille: 300,
            birth_food_units: 3,
            max_children: 3,
            founding_couples_percent: 40,
            child_ration_percent: 50,
            child_starve_days: 3,
            elder_speed_percent: 75,
            elder_work_percent: 125,
            arrivals_per_visit: 2,
            arrival_grant_coins: 60,
            arrival_food_per_head: 1,
            max_population: 160,
            names: [
                "Alba", "Beck", "Cora", "Dell", "Edda", "Fen", "Greta", "Hale", "Ilse", "Joss",
                "Kai", "Lark", "Maro", "Nora", "Odo", "Pell", "Quin", "Rue", "Sten", "Tilda",
                "Ula", "Vale", "Wyn", "Xan", "Yara", "Zed", "Arlo", "Bryn", "Cass", "Dov", "Elan",
                "Flo", "Gus", "Hana", "Ivar", "Juno", "Kell", "Lio", "Mina", "Ned",
            ]
            .into_iter()
            .map(str::to_owned)
            .collect(),
        }
    }
}

/// One crop residents sow on a field.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CropSpec {
    /// Stable key stored in saves.
    pub key: String,
    pub display: String,
    /// Goods one plot takes to sow.
    pub seed: Stack,
    /// Goods one ripe plot yields.
    pub harvest: Stack,
    /// Growing ticks from sowing to ripe at 100% growth.
    pub growth_ticks: u32,
    /// Seasons in which a plot may be sown. Nothing grows in winter.
    pub sow_seasons: Vec<Season>,
    /// Work ticks of one sowing, tending and harvest.
    pub sow_ticks: u32,
    pub tend_ticks: u32,
    pub harvest_ticks: u32,
    /// A plot wants tending this often; a neglected plot grows slower.
    pub tend_interval_ticks: u64,
    pub untended_growth_percent: u32,
    /// Farmers sow while the settlement holds, or has growing, fewer units of
    /// the harvest good than this per resident. 0 always sows.
    #[serde(default)]
    pub stock_per_resident: u32,
}

/// The built-in field crop. It is the homestead's grain definition (the same
/// seed, water-free field culture and harvest good) scaled to a field plot.
pub fn default_crops() -> Vec<CropSpec> {
    use crate::village::{catalog, homestead};
    vec![CropSpec {
        key: "grain".into(),
        display: "Grain".into(),
        seed: Stack {
            good: catalog::GRAIN.into(),
            count: homestead::SEED_COUNT,
        },
        harvest: Stack {
            good: catalog::GRAIN.into(),
            count: homestead::HARVEST_GRAIN * 2,
        },
        growth_ticks: homestead::GROW_TICKS * 8,
        sow_seasons: vec![Season::Spring, Season::Summer],
        sow_ticks: 160,
        tend_ticks: 120,
        harvest_ticks: 200,
        tend_interval_ticks: 24_000,
        untended_growth_percent: 60,
        stock_per_resident: 4,
    }]
}

/// Daily weather of the coastal world.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Reflect, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum Weather {
    #[default]
    Clear,
    Rain,
    Storm,
    Snow,
}
impl Weather {
    pub fn name(self) -> &'static str {
        match self {
            Self::Clear => "clear",
            Self::Rain => "rain",
            Self::Storm => "storm",
            Self::Snow => "snow",
        }
    }
}

/// A rare seeded event that lasts one season.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Reflect)]
#[serde(rename_all = "snake_case")]
pub enum Shock {
    Drought,
    Blight,
    HarshWinter,
}
impl Shock {
    pub fn name(self) -> &'static str {
        match self {
            Self::Drought => "drought",
            Self::Blight => "blight",
            Self::HarshWinter => "harsh winter",
        }
    }
}

/// Relative odds of each kind of day in one season.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WeatherOdds {
    pub clear: u32,
    pub rain: u32,
    pub storm: u32,
    pub snow: u32,
}
impl WeatherOdds {
    pub fn total(&self) -> u32 {
        self.clear
            .saturating_add(self.rain)
            .saturating_add(self.storm)
            .saturating_add(self.snow)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct WeatherSpec {
    pub enabled: bool,
    pub spring: WeatherOdds,
    pub summer: WeatherOdds,
    pub autumn: WeatherOdds,
    pub winter: WeatherOdds,
    /// Days of forecast the read accessor returns, today excluded.
    pub forecast_days: u32,
    /// Crop growth on a rainy day, percent of a clear day.
    pub rain_growth_percent: u32,
    /// Units every water source gains per 600 ticks of rain.
    pub rain_water_units: u32,
    /// Warmth lost in the open on a snowy day, percent of an ordinary cold day.
    pub snow_warmth_loss_percent: u32,
    /// Chance per summer of a drought, per spring or summer of a blight and per
    /// winter of a harsh winter.
    pub drought_permille: u32,
    pub blight_permille: u32,
    pub harsh_winter_permille: u32,
    /// No event in this many seasons after the founding.
    pub shock_grace_seasons: u32,
    pub drought_growth_percent: u32,
    /// Share of the growing plots a blight destroys.
    pub blight_loss_percent: u32,
    pub harsh_warmth_loss_percent: u32,
    /// Extra hearth logs a home burns per day in a harsh winter.
    pub harsh_extra_hearth_lightings: u32,
}

impl Default for WeatherSpec {
    fn default() -> Self {
        let odds = |clear, rain, storm, snow| WeatherOdds {
            clear,
            rain,
            storm,
            snow,
        };
        Self {
            enabled: true,
            spring: odds(600, 330, 60, 10),
            summer: odds(700, 240, 60, 0),
            autumn: odds(560, 330, 90, 20),
            winter: odds(520, 50, 70, 360),
            forecast_days: 3,
            rain_growth_percent: 150,
            rain_water_units: 4,
            snow_warmth_loss_percent: 150,
            drought_permille: 100,
            blight_permille: 80,
            harsh_winter_permille: 120,
            shock_grace_seasons: 3,
            drought_growth_percent: 50,
            blight_loss_percent: 40,
            harsh_warmth_loss_percent: 150,
            harsh_extra_hearth_lightings: 1,
        }
    }
}

impl WeatherSpec {
    pub fn odds(&self, season: Season) -> WeatherOdds {
        match season {
            Season::Spring => self.spring,
            Season::Summer => self.summer,
            Season::Autumn => self.autumn,
            Season::Winter => self.winter,
        }
    }
}

/// What the boat pays for one good of the island.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MerchantBuy {
    pub good: String,
    /// Most units bought per visit.
    pub max_units: u32,
    /// Price paid, percent of the good's base price.
    pub price_percent: u32,
    /// Units the stores keep: the boat buys only what lies above this.
    #[serde(default)]
    pub keep_units: u32,
}

/// What the boat brings of one good.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MerchantSell {
    pub good: String,
    /// Units aboard at each visit.
    pub units: u32,
    /// Price asked, percent of the good's base price.
    pub price_percent: u32,
    /// The civic fund buys while the port settlement holds fewer units.
    pub want_units: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct MerchantSpec {
    pub enabled: bool,
    /// Key of the settlement the boat calls at.
    pub settlement: String,
    /// Building (a stall) founded there as the trading post.
    pub post: String,
    /// Building (a stall) founded near the port's centre as its market: the
    /// boat's imports are laid and sold there, where people meet, while the
    /// trading post at the water takes the goods the boat buys. Empty: the
    /// imports land at the post.
    pub market: String,
    /// Days between two visits, and days from the founding to the first.
    pub interval_days: u64,
    pub first_visit_day: u64,
    /// Permille of the day at which the boat lands, and ticks it stays.
    pub arrive_permille: u32,
    pub stay_ticks: u64,
    /// Coins the merchant owns at the founding. Nothing refills it: the purse
    /// only changes by trade with the island.
    pub purse: u32,
    pub buys: Vec<MerchantBuy>,
    pub sells: Vec<MerchantSell>,
    /// At each call the boat announces what it will buy at the next one: this
    /// many goods of `buys` (never fiber or thread), rotating over those the
    /// island's trades make; 0 announces nothing.
    pub order_goods: u32,
    /// Units of each announced good.
    pub order_units: u32,
    /// Price of an announced good, percent of the island's quote (at least
    /// one coin above it): the order is worth making for.
    pub order_price_percent: u32,
    /// Net coins the boat may take from the island in a year, percent of the
    /// island's coins (closing fix): in one visit it sells no more than it
    /// bought plus this year's share spread over the visits, so imports
    /// cannot drain the island. 100 lifts the cap.
    pub trade_balance_percent: u32,
    /// How the outside world's prices and the boat's hold change between
    /// visits (review fix S2.3). Absent: a fixed boat.
    #[serde(default)]
    pub drift: MerchantDrift,
}

/// Seeded drift of the boat's outside prices and of what it brings.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct MerchantDrift {
    /// Largest random move of a good's outside price per visit, permille.
    pub price_step_permille: u32,
    /// Bounds of the outside price factor, permille.
    pub price_min_permille: u32,
    pub price_max_permille: u32,
    /// Pull back towards 1000 per visit: `(1000 - factor) / revert_divisor`.
    pub revert_divisor: u32,
    /// Share of a sell entry's `units` aboard, percent.
    pub hold_min_percent: u32,
    pub hold_max_percent: u32,
    /// Chance a sell good is not aboard at all, permille.
    pub missing_permille: u32,
}
impl Default for MerchantDrift {
    /// A fixed boat: no drift, the full hold every visit.
    fn default() -> Self {
        Self {
            price_step_permille: 0,
            price_min_permille: 1000,
            price_max_permille: 1000,
            revert_divisor: 8,
            hold_min_percent: 100,
            hold_max_percent: 100,
            missing_permille: 0,
        }
    }
}
impl MerchantDrift {
    /// The shipped boat's drift.
    pub fn shipped() -> Self {
        Self {
            price_step_permille: 60,
            price_min_permille: 700,
            price_max_permille: 1400,
            revert_divisor: 8,
            hold_min_percent: 60,
            hold_max_percent: 140,
            missing_permille: 120,
        }
    }
}

impl Default for MerchantSpec {
    fn default() -> Self {
        use crate::village::catalog;
        let buy = |good: &str, max_units, price_percent, keep_units| MerchantBuy {
            good: good.into(),
            max_units,
            price_percent,
            keep_units,
        };
        let sell = |good: &str, units, price_percent, want_units| MerchantSell {
            good: good.into(),
            units,
            price_percent,
            want_units,
        };
        Self {
            enabled: true,
            settlement: "harbour".into(),
            post: "trading_post".into(),
            market: "market_stall".into(),
            interval_days: 3,
            first_visit_day: 2,
            arrive_permille: 150,
            stay_ticks: 9_600,
            // Sized so a visit trades roughly 5 to 10% of a season's island
            // trade: it buys the surplus above the stock targets and sells
            // what the island is short of.
            purse: 6_000,
            buys: vec![
                buy("log", 60, 100, 8),
                buy("plank", 40, 100, 8),
                buy(catalog::GRAIN, 60, 90, 40),
                // Not fiber or thread: fiber is a harvest crop the tailors
                // spin through winter and spring; a boat buying it in autumn
                // left seed 23 with no coat made in the last spring days.
                buy(catalog::LEATHER, 12, 100, 2),
                buy(catalog::FISH, 20, 100, 4),
                buy(catalog::BOWL, 10, 100, 2),
                buy(catalog::BERRIES, 30, 100, 8),
                buy(catalog::HIDE, 12, 100, 2),
            ],
            sells: vec![
                sell(catalog::GRAIN, 40, 140, 16),
                sell(catalog::FLOUR, 12, 140, 4),
                sell("ingot", 8, 140, 3),
                sell("log", 12, 140, 8),
                sell("bread", 16, 140, 4),
            ],
            order_goods: 3,
            order_units: 10,
            order_price_percent: 130,
            trade_balance_percent: 5,
            drift: MerchantDrift::shipped(),
        }
    }
}

impl DemographySpec {
    pub(super) fn validate(&self) -> Result<(), String> {
        let d = self;
        let checks = [
            (
                d.adult_age_seasons >= 1
                    && d.adult_age_seasons < d.elder_age_seasons
                    && d.elder_age_seasons <= d.lifespan_min_seasons
                    && d.lifespan_min_seasons <= d.lifespan_max_seasons
                    && d.lifespan_max_seasons <= 4_000,
                "needs 1 <= adult_age_seasons < elder_age_seasons <= lifespan_min_seasons <= \
                 lifespan_max_seasons <= 4000",
            ),
            (
                (d.adult_age_seasons..d.lifespan_min_seasons).contains(&d.founder_max_age_seasons),
                "founder_max_age_seasons must lie between adult_age_seasons and \
                 lifespan_min_seasons",
            ),
            (
                (1..=100_000).contains(&d.pair_affinity) && d.courtship_days <= 1_000,
                "pair_affinity must be 1..=100000 and courtship_days at most 1000",
            ),
            (
                d.birth_chance_permille <= 1_000
                    && d.birth_cooldown_days <= 10_000
                    && d.birth_food_units <= 64
                    && d.max_children <= 8
                    && d.founding_couples_percent <= 100,
                "birth_chance_permille must be 0..=1000, birth_cooldown_days at most 10000, \
                 birth_food_units at most 64, max_children at most 8 and                  founding_couples_percent at most 100",
            ),
            (
                d.child_ration_percent <= 200 && (1..=1_000).contains(&d.child_starve_days),
                "child_ration_percent must be 0..=200 and child_starve_days 1..=1000",
            ),
            (
                (10..=100).contains(&d.elder_speed_percent)
                    && (100..=400).contains(&d.elder_work_percent),
                "elder_speed_percent must be 10..=100 and elder_work_percent 100..=400",
            ),
            (
                d.arrivals_per_visit <= 12
                    && d.arrival_grant_coins <= 10_000
                    && d.arrival_food_per_head <= 64
                    && (1..=2_000).contains(&d.max_population),
                "arrivals_per_visit must be at most 12, arrival_grant_coins at most 10000, \
                 arrival_food_per_head at most 64 and max_population 1..=2000",
            ),
            (
                !d.names.is_empty()
                    && d.names.len() <= 512
                    && d.names
                        .iter()
                        .all(|name| !name.trim().is_empty() && name.len() <= 32),
                "names needs 1..=512 non-empty names of at most 32 bytes",
            ),
        ];
        for (ok, message) in checks {
            if !ok {
                return Err(format!("economy: demography {message}"));
            }
        }
        Ok(())
    }

    pub(super) fn sim_hash(&self, h: &mut Fnv64) {
        h.u8(u8::from(self.enabled));
        for value in [
            self.adult_age_seasons,
            self.elder_age_seasons,
            self.lifespan_min_seasons,
            self.lifespan_max_seasons,
            self.founder_max_age_seasons,
            self.courtship_days,
            self.birth_cooldown_days,
            self.birth_chance_permille,
            self.birth_food_units,
            self.max_children,
            self.founding_couples_percent,
            self.child_ration_percent,
            self.child_starve_days,
            self.elder_speed_percent,
            self.elder_work_percent,
            self.arrivals_per_visit,
            self.arrival_grant_coins,
            self.arrival_food_per_head,
            self.max_population,
        ] {
            h.u32(value);
        }
        h.i32(self.pair_affinity);
        h.u32(self.names.len() as u32);
        for name in &self.names {
            h.str(name);
        }
    }
}

pub(super) fn validate_crops(economy: &EconomySpec) -> Result<(), String> {
    if economy.crops.len() > 16 {
        return Err("economy: crops needs at most 16 entries".into());
    }
    let mut seen = std::collections::BTreeSet::new();
    for crop in &economy.crops {
        let name = &crop.key;
        if name.trim().is_empty() || !seen.insert(name.as_str()) {
            return Err(format!(
                "economy: crop `{name}` has an empty or duplicate key"
            ));
        }
        if crop.display.trim().is_empty() {
            return Err(format!("economy: crop `{name}` needs a display name"));
        }
        for (stack, what) in [(&crop.seed, "seed"), (&crop.harvest, "harvest")] {
            if economy.good(&stack.good).is_none() {
                return Err(format!(
                    "economy: crop `{name}` {what} references unknown good `{}`",
                    stack.good
                ));
            }
            if !(1..=100).contains(&stack.count) {
                return Err(format!(
                    "economy: crop `{name}` {what} count must be 1..=100"
                ));
            }
        }
        if !(100..=2_000_000).contains(&crop.growth_ticks) {
            return Err(format!(
                "economy: crop `{name}` growth_ticks must be 100..=2000000"
            ));
        }
        if crop.sow_seasons.is_empty() || crop.sow_seasons.contains(&Season::Winter) {
            return Err(format!(
                "economy: crop `{name}` needs at least one sow season, and winter is not one"
            ));
        }
        if [crop.sow_ticks, crop.tend_ticks, crop.harvest_ticks]
            .iter()
            .any(|ticks| !(20..=10_000).contains(ticks))
        {
            return Err(format!(
                "economy: crop `{name}` sow_ticks, tend_ticks and harvest_ticks must be 20..=10000"
            ));
        }
        if !(600..=2_000_000).contains(&crop.tend_interval_ticks)
            || !(1..=100).contains(&crop.untended_growth_percent)
            || crop.stock_per_resident > 1_000
        {
            return Err(format!(
                "economy: crop `{name}` needs tend_interval_ticks 600..=2000000, \
                 untended_growth_percent 1..=100 and stock_per_resident at most 1000"
            ));
        }
    }
    Ok(())
}

pub(super) fn hash_crops(crops: &[CropSpec], h: &mut Fnv64) {
    h.u32(crops.len() as u32);
    for crop in crops {
        h.str(&crop.key);
        h.str(&crop.display);
        for stack in [&crop.seed, &crop.harvest] {
            h.str(&stack.good);
            h.u32(stack.count);
        }
        h.u32(crop.growth_ticks);
        h.u32(crop.sow_seasons.len() as u32);
        for season in &crop.sow_seasons {
            h.u8(season_tag(*season));
        }
        h.u32(crop.sow_ticks);
        h.u32(crop.tend_ticks);
        h.u32(crop.harvest_ticks);
        h.u64(crop.tend_interval_ticks);
        h.u32(crop.untended_growth_percent);
        h.u32(crop.stock_per_resident);
    }
}

impl WeatherSpec {
    pub(super) fn validate(&self) -> Result<(), String> {
        let w = self;
        for (name, odds) in [
            ("spring", w.spring),
            ("summer", w.summer),
            ("autumn", w.autumn),
            ("winter", w.winter),
        ] {
            if odds.total() == 0 || odds.total() > 1_000_000 {
                return Err(format!(
                    "economy: weather.{name} needs odds that sum to 1..=1000000"
                ));
            }
        }
        let checks = [
            (w.forecast_days <= 14, "forecast_days must be at most 14"),
            (
                (0..=400).contains(&w.rain_growth_percent) && w.rain_water_units <= 1_000,
                "rain_growth_percent must be 0..=400 and rain_water_units at most 1000",
            ),
            (
                (100..=1_000).contains(&w.snow_warmth_loss_percent)
                    && (100..=1_000).contains(&w.harsh_warmth_loss_percent)
                    && w.harsh_extra_hearth_lightings <= 12,
                "snow_warmth_loss_percent and harsh_warmth_loss_percent must be 100..=1000 and \
                 harsh_extra_hearth_lightings at most 12",
            ),
            (
                w.drought_permille.saturating_add(w.blight_permille) <= 1_000
                    && w.harsh_winter_permille <= 1_000
                    && w.shock_grace_seasons <= 400,
                "drought_permille plus blight_permille and harsh_winter_permille must each be at \
                 most 1000, and shock_grace_seasons at most 400",
            ),
            (
                w.drought_growth_percent <= 100 && w.blight_loss_percent <= 100,
                "drought_growth_percent and blight_loss_percent must be 0..=100",
            ),
        ];
        for (ok, message) in checks {
            if !ok {
                return Err(format!("economy: weather.{message}"));
            }
        }
        Ok(())
    }

    pub(super) fn sim_hash(&self, h: &mut Fnv64) {
        h.u8(u8::from(self.enabled));
        for odds in [self.spring, self.summer, self.autumn, self.winter] {
            for value in [odds.clear, odds.rain, odds.storm, odds.snow] {
                h.u32(value);
            }
        }
        for value in [
            self.forecast_days,
            self.rain_growth_percent,
            self.rain_water_units,
            self.snow_warmth_loss_percent,
            self.drought_permille,
            self.blight_permille,
            self.harsh_winter_permille,
            self.shock_grace_seasons,
            self.drought_growth_percent,
            self.blight_loss_percent,
            self.harsh_warmth_loss_percent,
            self.harsh_extra_hearth_lightings,
        ] {
            h.u32(value);
        }
    }
}

pub(super) fn validate_merchant(economy: &EconomySpec) -> Result<(), String> {
    let m = &economy.merchant;
    if !m.enabled {
        return Ok(());
    }
    if m.settlement.trim().is_empty() {
        return Err("economy: merchant.settlement needs the key of a settlement".into());
    }
    match economy.building(&m.post).map(|def| &def.kind) {
        Some(super::building_spec::BuildingKind::Stall { .. }) => {}
        _ => {
            return Err(format!(
                "economy: merchant.post `{}` must be a building of type stall",
                m.post
            ));
        }
    }
    if !m.market.is_empty()
        && !matches!(
            economy.building(&m.market).map(|def| &def.kind),
            Some(super::building_spec::BuildingKind::Stall { .. })
        )
    {
        return Err(format!(
            "economy: merchant.market `{}` must be empty or a building of type stall",
            m.market
        ));
    }
    if !(1..=400).contains(&m.interval_days)
        || m.first_visit_day > 400
        || m.arrive_permille >= 700
        || !(600..=48_000).contains(&m.stay_ticks)
        || m.purse > 10_000_000
    {
        return Err(
            "economy: merchant needs interval_days 1..=400, first_visit_day at most 400, \
             arrive_permille below 700, stay_ticks 600..=48000 and purse at most 10000000"
                .into(),
        );
    }
    if m.order_goods > 16
        || m.order_units > 1_000
        || !(101..=1_000).contains(&m.order_price_percent)
        || m.trade_balance_percent > 100
    {
        return Err(
            "economy: merchant needs order_goods at most 16, order_units at most 1000 and              order_price_percent 101..=1000, trade_balance_percent at most 100"
                .into(),
        );
    }
    let d = &m.drift;
    if d.price_step_permille > 300
        || !(100..=1000).contains(&d.price_min_permille)
        || !(1000..=5000).contains(&d.price_max_permille)
        || !(1..=100).contains(&d.revert_divisor)
        || d.hold_min_percent > 100
        || !(100..=400).contains(&d.hold_max_percent)
        || d.missing_permille > 500
    {
        return Err("economy: merchant.drift needs price_step_permille at most 300, price_min_permille 100..=1000, price_max_permille 1000..=5000, revert_divisor 1..=100, hold_min_percent at most 100, hold_max_percent 100..=400 and missing_permille at most 500".into());
    }
    if m.buys.len() > 64 || m.sells.len() > 64 {
        return Err("economy: merchant.buys and merchant.sells take at most 64 entries".into());
    }
    let mut seen = std::collections::BTreeSet::new();
    for buy in &m.buys {
        if !super::building_spec::storable(economy, &buy.good) {
            return Err(format!(
                "economy: merchant.buys `{}` is unknown, a tool or clothing; the boat trades only \
                 goods a store can hold",
                buy.good
            ));
        }
        if !seen.insert(buy.good.as_str())
            || !(1..=10_000).contains(&buy.max_units)
            || !(1..=1_000).contains(&buy.price_percent)
            || buy.keep_units > 10_000
        {
            return Err(format!(
                "economy: merchant.buys `{}` is listed twice or needs max_units 1..=10000, \
                 price_percent 1..=1000 and keep_units at most 10000",
                buy.good
            ));
        }
    }
    seen.clear();
    for sell in &m.sells {
        if !super::building_spec::storable(economy, &sell.good) {
            return Err(format!(
                "economy: merchant.sells `{}` is unknown, a tool or clothing; the boat trades \
                 only goods a store can hold",
                sell.good
            ));
        }
        if !seen.insert(sell.good.as_str())
            || !(1..=10_000).contains(&sell.units)
            || !(1..=1_000).contains(&sell.price_percent)
            || sell.want_units > 10_000
        {
            return Err(format!(
                "economy: merchant.sells `{}` is listed twice or needs units 1..=10000, \
                 price_percent 1..=1000 and want_units at most 10000",
                sell.good
            ));
        }
    }
    Ok(())
}

impl MerchantSpec {
    pub(super) fn sim_hash(&self, h: &mut Fnv64) {
        h.u8(u8::from(self.enabled));
        h.str(&self.settlement);
        h.str(&self.post);
        h.str(&self.market);
        h.u64(self.interval_days);
        h.u64(self.first_visit_day);
        h.u32(self.arrive_permille);
        h.u64(self.stay_ticks);
        h.u32(self.purse);
        h.u32(self.buys.len() as u32);
        for buy in &self.buys {
            h.str(&buy.good);
            h.u32(buy.max_units);
            h.u32(buy.price_percent);
            h.u32(buy.keep_units);
        }
        h.u32(self.sells.len() as u32);
        for sell in &self.sells {
            h.str(&sell.good);
            h.u32(sell.units);
            h.u32(sell.price_percent);
            h.u32(sell.want_units);
        }
        h.u32(self.order_goods);
        h.u32(self.order_units);
        h.u32(self.order_price_percent);
        h.u32(self.trade_balance_percent);
        let d = &self.drift;
        for value in [
            d.price_step_permille,
            d.price_min_permille,
            d.price_max_permille,
            d.revert_divisor,
            d.hold_min_percent,
            d.hold_max_percent,
            d.missing_permille,
        ] {
            h.u32(value);
        }
    }
}

/// One tier of the bond between a played character and a resident.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct BondTier {
    pub name: String,
    /// The score the tier starts at, 0 to 100000.
    pub score: i32,
    /// Percent off when the played character buys from the resident.
    pub discount_percent: u32,
    /// The resident gives one unit of what it sells back when talked to, at
    /// most once in this many days. 0: never.
    pub gift_back_days: u64,
    /// The resident invites the played character to gatherings.
    pub invites: bool,
    /// The resident helps on the played character's homestead in its free
    /// time: a material for an open upgrade, work on it, or a ripe harvest.
    pub helps: bool,
}
impl Default for BondTier {
    fn default() -> Self {
        Self {
            name: "stranger".into(),
            score: 0,
            discount_percent: 0,
            gift_back_days: 0,
            invites: false,
            helps: false,
        }
    }
}

/// A gathering of the calendar: one day of a season, part of the day.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct GatheringSpec {
    pub key: String,
    pub name: String,
    /// `spring`, `summer`, `autumn` or `winter`.
    pub season: String,
    /// Day of the season, from 1; the last day when the season is shorter.
    pub day: u64,
    pub from_permille: u32,
    pub to_permille: u32,
    /// Morale each attendant gains, once.
    pub morale: i32,
}
impl Default for GatheringSpec {
    fn default() -> Self {
        Self {
            key: "feast".into(),
            name: "Feast".into(),
            season: "autumn".into(),
            day: 1,
            from_permille: 560,
            to_permille: 700,
            morale: 15_000,
        }
    }
}

/// One goal of the chain a played character works through.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct GoalSpec {
    pub key: String,
    pub title: String,
    /// `own_cottage`, `harvest`, `gift`, `friend`, `sleep`, `gathering`,
    /// `trade`, `talk`, `upgrade` or `coins`.
    pub kind: String,
    pub count: u32,
    /// For `friend`: the bond tier (index) that counts.
    pub tier: u32,
    /// For `homestead`: the homestead tier that must be built.
    pub homestead: String,
}
impl Default for GoalSpec {
    fn default() -> Self {
        Self {
            key: String::new(),
            title: String::new(),
            kind: "own_cottage".into(),
            count: 1,
            tier: 2,
            homestead: String::new(),
        }
    }
}

/// One upgrade of a homestead, paid in goods and work.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct HomesteadTier {
    pub key: String,
    pub name: String,
    /// The tier that must be built first; empty for none.
    pub after: String,
    /// The goal that must be reached first; empty for none.
    pub goal: String,
    /// Goods carried to the cottage and spent when the work is done.
    pub cost: Vec<super::economy_spec::Stack>,
    /// Ticks of work at the cottage.
    pub work_ticks: u32,
    /// Beds added to the cottage.
    pub beds: u32,
    /// Further plots the owner may hold.
    pub plots: u32,
    /// Gathering time of the owner in percent of the ordinary; 0 leaves it.
    pub gather_percent: u32,
    /// Living world: the catalogue home the cottage grows into (its body and
    /// beds); empty leaves the body.
    pub building: String,
    /// Units the homestead chest holds once this tier stands; 0 adds none.
    pub storage: u32,
}
impl Default for HomesteadTier {
    fn default() -> Self {
        Self {
            key: String::new(),
            name: String::new(),
            after: String::new(),
            goal: String::new(),
            cost: Vec::new(),
            work_ticks: 600,
            beds: 0,
            plots: 0,
            gather_percent: 0,
            building: String::new(),
            storage: 0,
        }
    }
}

/// Live mode: bonds, gifts, gatherings, goals and homestead tiers.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct LiveSpec {
    pub enabled: bool,
    /// Ascending by score; the first starts at 0.
    pub tiers: Vec<BondTier>,
    /// Points for the first talk, trade and gift of a day.
    pub talk_points: i32,
    pub trade_points: i32,
    pub gift_liked_points: i32,
    pub gift_plain_points: i32,
    /// Negative: a disliked gift costs.
    pub gift_disliked_points: i32,
    /// A bond without contact for this many days loses points every day.
    pub neglect_after_days: u64,
    pub neglect_points_per_day: i32,
    /// Fondness between residents fades by this much a day.
    pub resident_neglect_points_per_day: i32,
    /// Goods a resident may like or dislike as a gift: each likes one and
    /// dislikes another, by seed.
    pub gift_goods: Vec<String>,
    /// What a free resident says when it greets a played character
    /// (`life.greet player`, once a day each), by bond tier.
    pub greetings: Vec<String>,
    pub gatherings: Vec<GatheringSpec>,
    /// A resident goes to a gathering only while every need is above this.
    pub gathering_needs_above: i32,
    pub mourning_days: u64,
    pub mourning_morale: i32,
    pub goals: Vec<GoalSpec>,
    pub homestead: Vec<HomesteadTier>,
    /// A played character may sleep until morning from this part of the day.
    pub sleep_from_permille: u32,
    /// A friend at a helping tier helps only while every need is above this.
    pub help_needs_above: i32,
    /// Work ticks a friend's help adds to an upgrade.
    pub help_work_ticks: u32,
    /// Fee for filling a homestead buy order, percent of the goods' price,
    /// paid to the resident who brings them.
    pub haul_fee_percent: u32,
    /// Coins a hired builder earns for one stage of `build_stage_ticks`.
    pub builder_wage: u32,
}
impl Default for LiveSpec {
    fn default() -> Self {
        let tier = |name: &str, score, discount_percent, gift_back_days, invites| BondTier {
            name: name.into(),
            score,
            discount_percent,
            gift_back_days,
            invites,
            helps: score >= 20_000,
        };
        let goal = |key: &str, title: &str, kind: &str, count| GoalSpec {
            key: key.into(),
            title: title.into(),
            kind: kind.into(),
            count,
            tier: 2,
            homestead: String::new(),
        };
        let built = |key: &str, title: &str, tier: &str| GoalSpec {
            homestead: tier.into(),
            ..goal(key, title, "homestead", 1)
        };
        let stack = |good: &str, count| super::economy_spec::Stack {
            good: good.into(),
            count,
        };
        Self {
            enabled: true,
            tiers: vec![
                tier("stranger", 0, 0, 0, false),
                tier("acquaintance", 5_000, 0, 0, false),
                tier("friend", 20_000, 10, 4, false),
                tier("close friend", 50_000, 20, 2, true),
            ],
            talk_points: 1_000,
            trade_points: 1_500,
            gift_liked_points: 6_000,
            gift_plain_points: 2_000,
            gift_disliked_points: -2_000,
            neglect_after_days: 3,
            neglect_points_per_day: 500,
            resident_neglect_points_per_day: 25,
            gift_goods: vec![
                "bread".into(),
                "village_berries".into(),
                "village_cooked_meal".into(),
                "village_fish".into(),
                "log".into(),
                "plank".into(),
            ],
            greetings: vec![
                "Good day.".into(),
                "Good day to you. How is the homestead?".into(),
                "There you are! Come by when you have a moment.".into(),
                "I was hoping to see you today.".into(),
            ],
            gatherings: vec![
                GatheringSpec {
                    key: "harvest_feast".into(),
                    name: "Harvest feast".into(),
                    season: "autumn".into(),
                    day: 2,
                    from_permille: 560,
                    to_permille: 700,
                    morale: 15_000,
                },
                GatheringSpec {
                    key: "midwinter".into(),
                    name: "Midwinter fire".into(),
                    season: "winter".into(),
                    day: 4,
                    from_permille: 560,
                    to_permille: 700,
                    morale: 20_000,
                },
            ],
            gathering_needs_above: 50_000,
            help_needs_above: 40_000,
            help_work_ticks: 300,
            haul_fee_percent: 20,
            builder_wage: 4,
            mourning_days: 3,
            mourning_morale: 15_000,
            goals: vec![
                goal("cottage", "Build a cottage of your own", "own_cottage", 1),
                built("storage", "Build a storage chest", "storage"),
                built("second_plot", "Lay out a second plot", "second_plot"),
                built("better_tools", "Build a tool shed", "better_tools"),
                built("larger_cottage", "Enlarge the cottage", "larger_cottage"),
                goal("a_friend", "Make a friend who helps you", "friend", 1),
                goal("feast", "Attend a village gathering", "gathering", 1),
            ],
            homestead: vec![
                HomesteadTier {
                    key: "storage".into(),
                    name: "a storage chest".into(),
                    goal: "cottage".into(),
                    cost: vec![stack("plank", 4), stack("log", 2)],
                    work_ticks: 400,
                    storage: 40,
                    ..Default::default()
                },
                HomesteadTier {
                    key: "second_plot".into(),
                    name: "a second plot".into(),
                    after: "storage".into(),
                    cost: vec![stack("plank", 4), stack("log", 4)],
                    work_ticks: 600,
                    plots: 1,
                    ..Default::default()
                },
                HomesteadTier {
                    key: "better_tools".into(),
                    name: "a tool shed".into(),
                    after: "second_plot".into(),
                    cost: vec![stack("plank", 4), stack("ingot", 1)],
                    work_ticks: 600,
                    gather_percent: 80,
                    ..Default::default()
                },
                HomesteadTier {
                    key: "larger_cottage".into(),
                    name: "a larger cottage".into(),
                    after: "better_tools".into(),
                    cost: vec![stack("plank", 6), stack("log", 4)],
                    work_ticks: 900,
                    beds: 1,
                    building: "house".into(),
                    ..Default::default()
                },
            ],
            sleep_from_permille: 600,
        }
    }
}
impl LiveSpec {
    pub(super) fn sim_hash(&self, h: &mut Fnv64) {
        // Plain data in field order: its canonical text is its identity.
        h.str(&serde_json::to_string(self).unwrap_or_default());
    }
}

pub(super) fn validate_live(economy: &EconomySpec) -> Result<(), String> {
    let live = &economy.live;
    if !live.enabled {
        return Ok(());
    }
    let known = |good: &str| economy.goods.iter().any(|entry| entry.key == good);
    let ascending = live
        .tiers
        .windows(2)
        .all(|pair| pair[0].score < pair[1].score);
    if live.tiers.is_empty()
        || live.tiers[0].score != 0
        || !ascending
        || live.tiers.iter().any(|tier| {
            tier.name.is_empty()
                || !(0..=100_000).contains(&tier.score)
                || tier.discount_percent > 90
        })
    {
        return Err(
            "economy: live.tiers must be named, start at score 0, ascend to at most 100000 and discount at most 90 percent"
                .into(),
        );
    }
    if live.gift_goods.iter().any(|good| !known(good)) || live.sleep_from_permille >= 1000 {
        return Err(
            "economy: live.gift_goods must be goods of the list and sleep_from_permille below 1000"
                .into(),
        );
    }
    for gathering in &live.gatherings {
        if gathering.key.is_empty()
            || !["spring", "summer", "autumn", "winter"].contains(&gathering.season.as_str())
            || gathering.day == 0
            || gathering.from_permille >= gathering.to_permille
            || gathering.to_permille > 1000
            || !(0..=100_000).contains(&gathering.morale)
        {
            return Err(format!(
                "economy: live gathering `{}` needs a season, a day from 1 and from_permille below to_permille (at most 1000)",
                gathering.key
            ));
        }
    }
    for tier in &live.homestead {
        if tier.key.is_empty()
            || tier.name.is_empty()
            || tier
                .cost
                .iter()
                .any(|stack| !known(&stack.good) || stack.count == 0)
            || (!tier.after.is_empty()
                && !live.homestead.iter().any(|other| other.key == tier.after))
            || (!tier.goal.is_empty() && !live.goals.iter().any(|goal| goal.key == tier.goal))
            || tier.gather_percent > 100
            || tier.storage > 10_000
            || (!tier.building.is_empty()
                && !economy.buildings.iter().any(|building| {
                    building.id == tier.building
                        && matches!(
                            building.kind,
                            super::building_spec::BuildingKind::Home { .. }
                        )
                }))
        {
            return Err(format!(
                "economy: live homestead tier `{}` needs a name, known goods, a known `after` tier and goal, and gather_percent at most 100",
                tier.key
            ));
        }
    }
    if !(0..=100_000).contains(&live.gathering_needs_above)
        || !(0..=100_000).contains(&live.help_needs_above)
        || !(1..=24_000).contains(&live.help_work_ticks)
        || live.haul_fee_percent > 1000
        || live.builder_wage > 1000
    {
        return Err("economy: live.gathering_needs_above and help_needs_above must be 0..=100000, help_work_ticks 1..=24000, haul_fee_percent and builder_wage at most 1000".into());
    }
    const KINDS: [&str; 11] = [
        "own_cottage",
        "harvest",
        "gift",
        "friend",
        "sleep",
        "gathering",
        "trade",
        "talk",
        "upgrade",
        "coins",
        "homestead",
    ];
    if live.goals.iter().any(|goal| {
        goal.key.is_empty()
            || goal.count == 0
            || !KINDS.contains(&goal.kind.as_str())
            || (goal.kind == "homestead"
                && !live.homestead.iter().any(|tier| tier.key == goal.homestead))
    }) {
        return Err("economy: every live goal needs a key, a count and a known kind".into());
    }
    Ok(())
}
