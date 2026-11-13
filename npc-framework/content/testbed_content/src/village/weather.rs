//! Deterministic daily weather and rare seasonal shocks (regional profile).
//!
//! The weather of a day is a pure function of the world seed, the calendar
//! day and the `economy.weather` odds, so a forecast is exact and nothing is
//! drawn from a stateful generator. [`WeatherState`] only remembers what the
//! current day is, so a change is journaled once and the state hash covers it.
//! Every accessor answers "ordinary clear weather" outside the regional
//! profile, so older profiles are untouched.
use super::frontier::economy_spec;
use super::frontier::life_spec::WeatherSpec;
pub use super::frontier::life_spec::{Shock, Weather};
use super::{ResourcePatch, Season};
use bevy_app::App;
use bevy_ecs::prelude::*;
use bevy_reflect::Reflect;
use npc_framework::kernel::hash::SimHashResource;
use npc_framework::kernel::rng::sim_rand;
use npc_framework::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::json;

/// `sim_rand` purposes: the weather of a day and the shock of a season.
const WEATHER_PURPOSE: u32 = 0x5745_4101;
const SHOCK_PURPOSE: u32 = 0x5745_4102;
const HARSH_SNOW_PURPOSE: u32 = 0x5745_4103;
/// Rain tops up water sources this often.
const RAIN_ROUND_TICKS: u64 = 600;

/// The current day's weather. Derived from the seed; kept so that changes
/// are journaled once and saved worlds agree with their hash.
#[derive(Resource, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub struct WeatherState {
    /// Calendar day `today` belongs to; `None` before the first tick.
    pub day: Option<u64>,
    pub today: Weather,
    /// The shock of the running season, if any.
    pub shock: Option<Shock>,
    /// Days of each kind since the founding: clear, rain, storm, snow.
    pub days: [u32; 4],
    /// Shocks since the founding: drought, blight, harsh winter.
    pub shocks: [u32; 3],
}
impl SimHashResource for WeatherState {
    const NAME: &'static str = "regional_weather_v1";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.opt(self.day.as_ref(), |h, day| h.u64(*day));
        h.u8(self.today as u8);
        h.opt(self.shock.as_ref(), |h, shock| h.u8(*shock as u8));
        for count in self.days.iter().chain(&self.shocks) {
            h.u32(*count);
        }
    }
    fn snapshot_fns() -> Option<npc_framework::snapshot::ResourceSnapshotFns> {
        Some(npc_framework::snapshot::serde_resource_fns::<Self>())
    }
}

/// Regional profile only.
pub fn register(app: &mut App) {
    app.init_resource::<WeatherState>()
        .register_sim_resource::<WeatherState>();
}

fn spec(world: &World) -> Option<&WeatherSpec> {
    economy_spec::registry(world)
        .map(|registry| &registry.spec.weather)
        .filter(|weather| weather.enabled)
}

/// The shock of season number `season` (calendar day / days per season).
pub fn shock_of(spec: &WeatherSpec, seed: u64, season: u64, founding_season: u64) -> Option<Shock> {
    if season < founding_season.saturating_add(u64::from(spec.shock_grace_seasons)) {
        return None;
    }
    let roll = (sim_rand(seed, SimId(0), season, SHOCK_PURPOSE) % 1000) as u32;
    match season % 4 {
        0 => (roll < spec.blight_permille).then_some(Shock::Blight),
        1 => {
            if roll < spec.drought_permille {
                Some(Shock::Drought)
            } else if roll < spec.drought_permille.saturating_add(spec.blight_permille) {
                Some(Shock::Blight)
            } else {
                None
            }
        }
        3 => (roll < spec.harsh_winter_permille).then_some(Shock::HarshWinter),
        _ => None,
    }
}

/// The weather of calendar day `day`: pure in the seed, the day and the data.
pub fn weather_of(
    spec: &WeatherSpec,
    seed: u64,
    day: u64,
    season_days: u64,
    founding_season: u64,
) -> Weather {
    let season_days = season_days.max(1);
    let season = super::season_of(day, season_days);
    let odds = spec.odds(season);
    let total = u64::from(odds.total().max(1));
    let roll = (sim_rand(seed, SimId(0), day, WEATHER_PURPOSE) % total) as u32;
    let mut weather = if roll < odds.clear {
        Weather::Clear
    } else if roll < odds.clear + odds.rain {
        Weather::Rain
    } else if roll < odds.clear + odds.rain + odds.storm {
        Weather::Storm
    } else {
        Weather::Snow
    };
    match shock_of(spec, seed, day / season_days, founding_season) {
        // A drought has no rain; its storms are dry.
        Some(Shock::Drought) if weather == Weather::Rain => weather = Weather::Clear,
        Some(Shock::HarshWinter)
            if weather == Weather::Rain
                || (weather == Weather::Clear
                    && sim_rand(seed, SimId(0), day, HARSH_SNOW_PURPOSE).is_multiple_of(2)) =>
        {
            weather = Weather::Snow;
        }
        _ => {}
    }
    weather
}

fn context(world: &World) -> Option<(&WeatherSpec, u64, u64, u64)> {
    let spec = spec(world)?;
    let season_days = super::days_per_season(world);
    let founding_season = super::first_day(world) / season_days;
    Some((
        spec,
        world.resource::<WorldSeed>().0,
        season_days,
        founding_season,
    ))
}

/// Weather of any calendar day of this world; clear without regional weather.
pub fn on_day(world: &World, day: u64) -> Weather {
    context(world).map_or(Weather::Clear, |(spec, seed, season_days, founding)| {
        weather_of(spec, seed, day, season_days, founding)
    })
}

/// Today's weather: the noted state, which the first tick of a day refreshes.
pub fn today(world: &World) -> Weather {
    match world.get_resource::<WeatherState>() {
        Some(state) if state.day.is_some() => state.today,
        Some(_) => on_day(world, super::calendar(world).0),
        None => Weather::Clear,
    }
}

/// The shock of the running season.
pub fn shock(world: &World) -> Option<Shock> {
    match world.get_resource::<WeatherState>() {
        Some(state) if state.day.is_some() => state.shock,
        Some(_) => shock_on(world, super::calendar(world).0),
        None => None,
    }
}

fn shock_on(world: &World, day: u64) -> Option<Shock> {
    let (spec, seed, season_days, founding) = context(world)?;
    shock_of(spec, seed, day / season_days, founding)
}

/// The next days' weather, tomorrow first: `(calendar day, weather)`.
pub fn forecast(world: &World) -> Vec<(u64, Weather)> {
    let Some((spec, ..)) = context(world) else {
        return Vec::new();
    };
    let today = super::calendar(world).0;
    (1..=u64::from(spec.forecast_days))
        .map(|ahead| (today + ahead, on_day(world, today + ahead)))
        .collect()
}

pub fn storm(world: &World) -> bool {
    today(world) == Weather::Storm
}

/// Whether tomorrow brings a storm: the forecast is exact.
pub fn storm_tomorrow(world: &World) -> bool {
    world.get_resource::<WeatherState>().is_some()
        && on_day(world, super::calendar(world).0 + 1) == Weather::Storm
}

pub fn raining(world: &World) -> bool {
    today(world) == Weather::Rain
}

pub fn snowing(world: &World) -> bool {
    today(world) == Weather::Snow
}

/// Crop growth today in percent of a clear day: rain helps, a drought hurts,
/// and nothing grows in winter or under snow.
pub fn growth_percent(world: &World) -> u32 {
    let Some((spec, ..)) = context(world) else {
        return 100;
    };
    if super::calendar(world).1 == Season::Winter {
        return 0;
    }
    let base = match today(world) {
        Weather::Rain => spec.rain_growth_percent,
        Weather::Snow => 0,
        Weather::Clear | Weather::Storm => 100,
    };
    if shock(world) == Some(Shock::Drought) {
        base * spec.drought_growth_percent / 100
    } else {
        base
    }
}

/// Warmth lost in the open today, percent of an ordinary cold day.
pub fn warmth_loss_percent(world: &World) -> u32 {
    let Some((spec, ..)) = context(world) else {
        return 100;
    };
    let snow = if snowing(world) {
        spec.snow_warmth_loss_percent
    } else {
        100
    };
    if shock(world) == Some(Shock::HarshWinter) {
        snow * spec.harsh_warmth_loss_percent / 100
    } else {
        snow
    }
}

/// Extra hearth logs a home burns per winter day in a harsh winter.
pub fn extra_hearth_lightings(world: &World) -> u32 {
    match (context(world), shock(world)) {
        (Some((spec, ..)), Some(Shock::HarshWinter)) => spec.harsh_extra_hearth_lightings,
        _ => 0,
    }
}

fn index(weather: Weather) -> usize {
    match weather {
        Weather::Clear => 0,
        Weather::Rain => 1,
        Weather::Storm => 2,
        Weather::Snow => 3,
    }
}

/// Once per tick in the regional Effects pass: notes a new day's weather and
/// a new season's shock in the journal, and lets rain refill water sources.
pub fn tick(world: &mut World) {
    if spec(world).is_none() {
        return;
    }
    let now = world.resource::<SimClock>().tick;
    let (day, season, _) = super::calendar(world);
    if world.resource::<WeatherState>().day != Some(day) {
        let weather = on_day(world, day);
        let shock = shock_on(world, day);
        let (previous, previous_shock, first) = {
            let state = world.resource::<WeatherState>();
            (state.today, state.shock, state.day.is_none())
        };
        let season_days = super::days_per_season(world);
        let new_season = first || day.is_multiple_of(season_days);
        {
            let mut state = world.resource_mut::<WeatherState>();
            state.day = Some(day);
            state.today = weather;
            state.shock = shock;
            state.days[index(weather)] += 1;
            if new_season && let Some(shock) = shock {
                state.shocks[shock as usize] += 1;
            }
        }
        if first || weather != previous {
            let forecast: Vec<_> = forecast(world)
                .into_iter()
                .map(|(_, weather)| weather.name())
                .collect();
            record_effect(
                world,
                None,
                "VillageWeather",
                &json!({"day":day + 1,"weather":weather.name(),
                    "previous":(!first).then(|| previous.name()),"season":season,
                    "forecast":forecast,"growth_percent":growth_percent(world),
                    "warmth_loss_percent":warmth_loss_percent(world)}),
            );
        }
        if new_season && (shock.is_some() || previous_shock.is_some()) && shock != previous_shock {
            record_effect(
                world,
                None,
                "VillageSeasonShock",
                &json!({"day":day + 1,"season":season,"shock":shock.map(Shock::name),
                    "ended":previous_shock.map(Shock::name)}),
            );
        }
        if new_season && shock == Some(Shock::Blight) {
            super::fields::blight(world, day);
        }
    }
    if now.is_multiple_of(RAIN_ROUND_TICKS) && raining(world) {
        let units = spec(world).map_or(0, |spec| spec.rain_water_units);
        let mut sources: Vec<_> = world
            .query::<(Entity, &SimId, &ResourcePatch)>()
            .iter(world)
            .filter(|(_, _, patch)| {
                patch.kind == super::roles::roles(world).drink && patch.remaining < patch.capacity
            })
            .map(|(entity, id, _)| (*id, entity))
            .collect();
        sources.sort_unstable_by_key(|(id, _)| *id);
        for (_, entity) in sources {
            super::frontier::ecology::replenish(world, entity, units);
        }
    }
}
