//! Data-driven geographical campaign, independent of renderer and battle rules.
use crate::catalog::{self, ContentError};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub struct Modifiers {
    pub enemy_health: f32,
    pub enemy_speed: f32,
    pub reward: f32,
    pub supply: i32,
    pub personal_damage: f32,
    pub npc_skill: i32,
}
impl Default for Modifiers {
    fn default() -> Self {
        Self {
            enemy_health: 1.0,
            enemy_speed: 1.0,
            reward: 1.0,
            supply: 0,
            personal_damage: 1.0,
            npc_skill: 0,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct StageThemeDefinition {
    pub id: String,
    pub title: String,
    pub setting: String,
    pub weather: String,
    pub lighting: String,
    pub props: Vec<String>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct StageTheme {
    pub id: String,
    pub title: String,
    pub biome: String,
    pub setting: String,
    pub weather: String,
    pub lighting: String,
    pub props: Vec<String>,
    pub variation: u64,
}
fn default_stage_themes() -> Vec<StageThemeDefinition> {
    [
        (
            "roadside",
            "Abandoned service road",
            "road",
            "overcast",
            "dusk",
            ["road_sign", "abandoned_car", "utility_poles"],
        ),
        (
            "suburb",
            "Evacuation street",
            "suburb",
            "mist",
            "dawn",
            ["houses", "bus_stop", "sandbags"],
        ),
        (
            "depot",
            "Freight depot access",
            "depot",
            "light_rain",
            "overcast",
            ["containers", "loading_crane", "barrels"],
        ),
        (
            "industrial",
            "Foundry service yard",
            "industrial",
            "ash",
            "dusk",
            ["chimneys", "pipes", "scrap_piles"],
        ),
        (
            "checkpoint",
            "Quarantine checkpoint",
            "checkpoint",
            "clear",
            "night",
            ["watchtower", "floodlights", "barricades"],
        ),
        (
            "farmland",
            "Reclaimed farm track",
            "farmland",
            "ground_fog",
            "dawn",
            ["silos", "fences", "windmill"],
        ),
        (
            "railway",
            "Silent rail crossing",
            "railway",
            "drizzle",
            "overcast",
            ["rail_tracks", "signal_box", "carriages"],
        ),
        (
            "waterfront",
            "Harbor evacuation gate",
            "waterfront",
            "sea_mist",
            "dusk",
            ["dock_crane", "mooring_posts", "cargo_nets"],
        ),
        (
            "market",
            "Empty market approach",
            "market",
            "dust",
            "sunset",
            ["stalls", "canopies", "parked_van"],
        ),
        (
            "highway",
            "Highway maintenance station",
            "highway",
            "wind",
            "dawn",
            ["overpass", "fuel_tank", "service_truck"],
        ),
    ]
    .into_iter()
    .map(
        |(id, title, setting, weather, lighting, props)| StageThemeDefinition {
            id: id.into(),
            title: title.into(),
            setting: setting.into(),
            weather: weather.into(),
            lighting: lighting.into(),
            props: props.into_iter().map(String::from).collect(),
        },
    )
    .collect()
}
fn stable_hash(text: &str) -> u64 {
    text.bytes().fold(0xcbf29ce484222325, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x100000001b3)
    })
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Country {
    pub id: String,
    pub name: String,
    pub population: u64,
    pub population_year: i32,
    pub polygons: Vec<Vec<[f32; 2]>>,
    pub center: [f32; 2],
    pub neighbors: Vec<String>,
    pub sea_neighbors: Vec<String>,
    pub biome: String,
    pub culture: String,
    pub region: String,
    pub kind: String,
    pub difficulty: f32,
    pub modifiers: Modifiers,
    pub boosts: Vec<String>,
    pub buffs: Vec<String>,
    pub debuffs: Vec<String>,
    pub level_override: Option<usize>,
    #[serde(default)]
    pub stage_theme_ids: Vec<String>,
}
impl Country {
    pub fn stage_theme(&self, stage: usize) -> StageTheme {
        self.stage_theme_with(stage, world())
    }
    pub fn stage_theme_with(&self, stage: usize, config: &WorldContent) -> StageTheme {
        let variation = stable_hash(&format!("{}:{stage}", self.id));
        let chosen = if self.stage_theme_ids.is_empty() {
            config.stage_themes.get(
                stage.wrapping_add(((variation >> 32) as usize % 2) * 5)
                    % config.stage_themes.len().max(1),
            )
        } else {
            let id = &self.stage_theme_ids[stage % self.stage_theme_ids.len()];
            config.stage_themes.iter().find(|t| &t.id == id)
        };
        let Some(theme) = chosen else {
            return StageTheme {
                biome: self.biome.clone(),
                setting: self.culture.clone(),
                variation,
                ..StageTheme::default()
            };
        };
        StageTheme {
            id: theme.id.clone(),
            title: format!("{} · {}", self.name, theme.title),
            biome: self.biome.clone(),
            setting: format!("{} · {}", self.culture, theme.setting),
            weather: theme.weather.clone(),
            lighting: theme.lighting.clone(),
            props: theme.props.clone(),
            variation,
        }
    }
    pub fn level_count(&self) -> usize {
        self.level_count_with(world())
    }
    pub fn level_count_with(&self, config: &WorldContent) -> usize {
        if let Some(count) = self.level_override {
            return count.clamp(1, config.max_levels.max(1));
        }
        let divisor = config.levels_per_population.max(1);
        self.population.div_ceil(divisor).clamp(
            config.min_levels as u64,
            config.max_levels.max(config.min_levels) as u64,
        ) as usize
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Source {
    pub url: String,
    pub sha256: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct StrategyConfig {
    pub recruitment_unlock_freed: usize,
    pub parallel_unlock_freed: usize,
    pub strategy_unlock_freed: usize,
    pub max_parallel: usize,
    pub max_roster: usize,
    pub recruit_cost: u64,
    pub outbreak_interval_ticks: u64,
    pub outbreak_duration_ticks: u64,
    pub outbreak_chance_percent: u32,
    pub guard_suppression_percent: u32,
    pub max_guards_per_country: usize,
}
impl Default for StrategyConfig {
    fn default() -> Self {
        Self {
            recruitment_unlock_freed: 1,
            parallel_unlock_freed: 2,
            strategy_unlock_freed: 4,
            max_parallel: 4,
            max_roster: 12,
            recruit_cost: 100,
            outbreak_interval_ticks: 36_000,
            outbreak_duration_ticks: 25_200,
            outbreak_chance_percent: 35,
            guard_suppression_percent: 30,
            max_guards_per_country: 3,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct WorldContent {
    pub version: u32,
    pub levels_per_population: u64,
    pub min_levels: usize,
    pub max_levels: usize,
    #[serde(default)]
    pub strategy_config: StrategyConfig,
    pub sources: Vec<Source>,
    pub countries: Vec<Country>,
    #[serde(default = "default_stage_themes")]
    pub stage_themes: Vec<StageThemeDefinition>,
}
impl WorldContent {
    pub fn country(&self, id: &str) -> Option<&Country> {
        self.countries.iter().find(|c| c.id == id)
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.version != 1
            || self.levels_per_population == 0
            || self.min_levels == 0
            || self.max_levels < self.min_levels
            || self.max_levels > 256
            || self.countries.is_empty()
            || self.countries.len() > 512
        {
            return Err("Invalid world level-count configuration".into());
        }
        let mut theme_ids = BTreeSet::new();
        if self.stage_themes.is_empty() || self.stage_themes.len() > 128 {
            return Err("A bounded nonempty stage-theme catalogue is required".into());
        }
        for theme in &self.stage_themes {
            if theme.id.trim().is_empty()
                || theme.id.len() > 48
                || !theme_ids.insert(theme.id.as_str())
                || [
                    &theme.title,
                    &theme.setting,
                    &theme.weather,
                    &theme.lighting,
                ]
                .into_iter()
                .any(|t| t.trim().is_empty() || t.len() > 120)
                || theme.props.len() > 16
                || theme
                    .props
                    .iter()
                    .any(|p| p.trim().is_empty() || p.len() > 48)
            {
                return Err(format!("Invalid stage theme {}", theme.id));
            }
        }
        let strategy = &self.strategy_config;
        if strategy.recruitment_unlock_freed == 0
            || strategy.parallel_unlock_freed < strategy.recruitment_unlock_freed
            || strategy.strategy_unlock_freed < strategy.parallel_unlock_freed
            || strategy.max_parallel == 0
            || strategy.max_parallel > 16
            || strategy.max_roster < strategy.max_parallel
            || strategy.max_roster > 100
            || strategy.outbreak_interval_ticks < 60
            || strategy.outbreak_duration_ticks == 0
            || strategy.outbreak_chance_percent > 100
            || strategy.guard_suppression_percent > 100
            || strategy.max_guards_per_country == 0
            || strategy.max_guards_per_country > strategy.max_roster
            || strategy.outbreak_interval_ticks > 60 * 60 * 24 * 60
            || strategy.outbreak_duration_ticks > 60 * 60 * 24 * 60
        {
            return Err("Invalid strategic campaign configuration".into());
        }
        let mut ids = BTreeSet::new();
        let mut geometry_points = 0usize;
        for c in &self.countries {
            if c.id.trim().is_empty() || c.id.len() > 48 || !ids.insert(c.id.as_str()) {
                return Err(format!("Duplicate or missing country ID {}", c.id));
            }
            if c.name.trim().is_empty()
                || c.name.len() > 120
                || c.biome.trim().is_empty()
                || c.culture.trim().is_empty()
                || c.center.iter().any(|v| !v.is_finite())
                || c.center[0].abs() > 180.01
                || c.center[1].abs() > 90.01
                || c.stage_theme_ids
                    .iter()
                    .any(|id| !theme_ids.contains(id.as_str()))
                || c.stage_theme_ids.len() > 256
                || c.biome.len() > 64
                || c.culture.len() > 256
                || c.region.len() > 120
                || c.kind.len() > 64
                || [&c.boosts, &c.buffs, &c.debuffs].into_iter().any(|list| {
                    list.len() > 16
                        || list
                            .iter()
                            .any(|text| text.trim().is_empty() || text.len() > 256)
                })
                || c.polygons.is_empty()
                || !c.difficulty.is_finite()
                || !(0.1..=10.0).contains(&c.difficulty)
            {
                return Err(format!("Invalid country {}", c.id));
            }
            let m = c.modifiers;
            if [m.enemy_health, m.enemy_speed, m.reward, m.personal_damage]
                .into_iter()
                .any(|v| !v.is_finite() || !(0.1..=10.0).contains(&v))
                || !(-100..=100).contains(&m.supply)
                || !(-10..=10).contains(&m.npc_skill)
            {
                return Err(format!("Invalid modifiers for {}", c.id));
            }
            if c.level_override
                .is_some_and(|n| n == 0 || n > self.max_levels)
            {
                return Err(format!("Invalid level override for {}", c.id));
            }
            for ring in &c.polygons {
                geometry_points = geometry_points
                    .checked_add(ring.len())
                    .ok_or("World geometry size overflows its supported limit")?;
                if ring.len() < 3
                    || ring.len() > 2048
                    || geometry_points > 200_000
                    || ring.iter().any(|p| {
                        !p[0].is_finite()
                            || !p[1].is_finite()
                            || p[0].abs() > 180.01
                            || p[1].abs() > 90.01
                    })
                {
                    return Err(format!("Invalid map geometry for {}", c.id));
                }
            }
        }
        for c in &self.countries {
            let mut seen = BTreeSet::new();
            for id in &c.neighbors {
                let n = self
                    .country(id)
                    .ok_or_else(|| format!("Unknown neighbor {} for {}", id, c.id))?;
                if id == &c.id || !seen.insert(id) || !n.neighbors.contains(&c.id) {
                    return Err(format!("Invalid/asymmetric neighbor {} / {}", c.id, id));
                }
            }
            if c.sea_neighbors.iter().any(|id| !c.neighbors.contains(id)) {
                return Err(format!("Sea route missing from neighbors for {}", c.id));
            }
        }
        // Every permitted starting country must be able to reach the same campaign.
        let mut reached = BTreeSet::new();
        let mut todo = vec![self.countries[0].id.as_str()];
        while let Some(id) = todo.pop() {
            if reached.insert(id) {
                for neighbor in &self.country(id).expect("validated neighbor").neighbors {
                    todo.push(neighbor.as_str());
                }
            }
        }
        if reached.len() != self.countries.len() {
            return Err("Country graph is disconnected; add explicit border or sea routes so every start can reach the campaign".into());
        }
        Ok(())
    }
}
impl WorldContent {
    /// Pure parse + validate, so tests can build alternative worlds in-process.
    pub fn from_json_str(text: &str) -> Result<Self, ContentError> {
        catalog::parse_validated("world", text, 32 * catalog::MIB, Self::validate)
    }
    /// Bundled default, or the `LASTLIGHT_WORLD` override.
    pub fn try_load() -> Result<Self, ContentError> {
        catalog::load(
            "world",
            32 * catalog::MIB,
            include_str!("world.json"),
            Self::from_json_str,
        )
    }
}
static WORLD: OnceLock<WorldContent> = OnceLock::new();
pub(crate) fn install() -> Result<&'static WorldContent, ContentError> {
    catalog::install(&WORLD, WorldContent::try_load)
}
pub fn world() -> &'static WorldContent {
    catalog::lazy(&WORLD, "world", WorldContent::try_load, || {
        WorldContent::from_json_str(include_str!("world.json"))
    })
}
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorldProgress {
    pub start: Option<String>,
    pub active: Option<String>,
    pub cleared: Vec<String>,
    pub country_levels: BTreeMap<String, usize>,
    /// Highest unique clear count per country; outbreaks cannot erase rank.
    #[serde(default)]
    pub clear_high_water: BTreeMap<String, usize>,
    #[serde(default)]
    pub unlocked: Vec<String>,
}
impl WorldProgress {
    pub fn active_country(&self) -> Option<&'static Country> {
        self.active.as_deref().and_then(|id| world().country(id))
    }
    pub fn completed(&self, id: &str) -> usize {
        self.country_levels.get(id).copied().unwrap_or(0)
    }
    pub fn next_stage(&self) -> usize {
        self.active_country()
            .map(|c| self.completed(&c.id).min(c.level_count().saturating_sub(1)))
            .unwrap_or(0)
    }
    pub fn choose_start(&mut self, id: &str) -> Result<(), String> {
        if self.start.is_some() {
            return Err("Starting country has already been chosen for this campaign".into());
        }
        if world().country(id).is_none() {
            return Err("Unknown starting country".into());
        }
        self.start = Some(id.into());
        self.active = Some(id.into());
        self.unlocked.push(id.into());
        Ok(())
    }
    pub fn can_enter(&self, id: &str) -> bool {
        let Some(country) = world().country(id) else {
            return false;
        };
        self.start.is_none()
            || self.unlocked.iter().any(|c| c == id)
            || self.country_levels.contains_key(id)
            || self.start.as_deref() == Some(id)
            || self.cleared.iter().any(|c| c == id)
            || country.neighbors.iter().any(|c| self.cleared.contains(c))
    }
    pub fn enter(&mut self, id: &str) -> Result<(), String> {
        if self.start.is_none() {
            return self.choose_start(id);
        }
        if !self.can_enter(id) {
            return Err("Clear a neighboring country to unlock this operation".into());
        }
        self.active = Some(id.into());
        if !self.unlocked.iter().any(|c| c == id) {
            self.unlocked.push(id.into());
        }
        Ok(())
    }
    /// Boss transactions call once with the attempted local stage. Replays are
    /// legal, but cannot clear a different stage or reopen neighboring rewards.
    /// Returns true only for the transaction that newly clears this country.
    pub fn record_clear(&mut self, stage: usize) -> Result<bool, String> {
        let id = self
            .active_country()
            .ok_or("Select a starting country first")?
            .id
            .clone();
        self.record_clear_in(&id, stage)
    }
    /// Records a stage clear for an explicit country, independent of `active`.
    pub fn record_clear_in(&mut self, id: &str, stage: usize) -> Result<bool, String> {
        let country = world().country(id).ok_or("Unknown country")?;
        let id = country.id.clone();
        let count = country.level_count();
        let completed = self.completed(&id);
        if stage >= count {
            return Err("Stage is outside this country".into());
        }
        if stage > completed {
            return Err("Clear the preceding local stage first".into());
        }
        if stage < completed || self.cleared.contains(&id) {
            return Ok(false);
        }
        self.country_levels.insert(id.clone(), completed + 1);
        self.clear_high_water
            .entry(id.clone())
            .and_modify(|value| *value = (*value).max(completed + 1))
            .or_insert(completed + 1);
        if completed + 1 == count {
            self.cleared.push(id);
            for neighbor in &country.neighbors {
                if !self.unlocked.contains(neighbor) {
                    self.unlocked.push(neighbor.clone());
                }
            }
            return Ok(true);
        }
        Ok(false)
    }
    /// An unresolved outbreak reopens only the final operation. Earned travel
    /// routes and historical first-clear rewards remain persistent.
    pub fn reopen(&mut self, id: &str) -> Result<bool, String> {
        let country = world().country(id).ok_or("Unknown outbreak country")?;
        if !self.cleared.iter().any(|c| c == id) {
            return Ok(false);
        }
        self.cleared.retain(|c| c != id);
        self.clear_high_water
            .entry(id.into())
            .and_modify(|value| *value = (*value).max(country.level_count()))
            .or_insert(country.level_count());
        self.country_levels
            .insert(id.into(), country.level_count().saturating_sub(1));
        if !self.unlocked.iter().any(|c| c == id) {
            self.unlocked.push(id.into());
        }
        Ok(true)
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.start.is_none() {
            if self.active.is_some()
                || !self.cleared.is_empty()
                || !self.country_levels.is_empty()
                || !self.clear_high_water.is_empty()
                || !self.unlocked.is_empty()
            {
                return Err("World progress has no starting country".into());
            }
            return Ok(());
        }
        if self
            .start
            .as_deref()
            .is_none_or(|id| world().country(id).is_none())
            || self.active.as_deref().is_none_or(|id| !self.can_enter(id))
        {
            return Err("Invalid active/start country in save".into());
        }
        let mut unlocked_ids = BTreeSet::new();
        for id in &self.unlocked {
            if world().country(id).is_none() || !unlocked_ids.insert(id) {
                return Err("Invalid unlocked-country ledger".into());
            }
        }
        let mut seen = BTreeSet::new();
        for id in &self.cleared {
            let c = world().country(id).ok_or("Unknown cleared country")?;
            if !seen.insert(id) || self.completed(id) != c.level_count() {
                return Err("Invalid completed country ledger".into());
            }
        }
        for (id, completed) in &self.country_levels {
            let c = world().country(id).ok_or("Unknown country progress")?;
            if *completed > c.level_count()
                || (*completed == c.level_count() && !self.cleared.contains(id))
            {
                return Err("Invalid local stage ledger".into());
            }
        }
        for (id, count) in &self.clear_high_water {
            if world().country(id).is_none_or(|c| *count > c.level_count())
                || *count < self.completed(id)
            {
                return Err("Invalid unique-clear high-water ledger".into());
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn catalogue_includes_small_states_and_valid_real_boundaries() {
        let data = world();
        data.validate().unwrap();
        assert_eq!(data.countries.len(), 242);
        for id in [
            "DEU", "USA", "CHN", "IND", "VAT", "MCO", "AND", "SMR", "LIE", "PSE", "TUV", "NRU",
        ] {
            let c = data
                .country(id)
                .expect("country selectable even at tiny map scale");
            assert!(!c.polygons.is_empty());
            assert!(!c.neighbors.is_empty(), "{id} must have a border/sea route");
        }
    }
    #[test]
    fn population_controls_level_counts_and_overrides_are_configurable() {
        let data = world();
        assert_eq!(data.country("VAT").unwrap().level_count(), 3);
        assert_eq!(data.country("DEU").unwrap().level_count(), 9);
        assert_eq!(data.country("IND").unwrap().level_count(), 24);
        let mut custom = data.clone();
        custom.levels_per_population = 50_000_000;
        assert_eq!(data.country("DEU").unwrap().level_count_with(&custom), 3);
        let mut c = data.country("VAT").unwrap().clone();
        c.level_override = Some(7);
        assert_eq!(c.level_count_with(&custom), 7);
    }
    #[test]
    fn start_can_be_any_country_but_not_reselected_for_free_unlocks() {
        for c in &world().countries {
            let mut p = WorldProgress::default();
            p.choose_start(&c.id).unwrap();
            assert_eq!(p.active_country().unwrap().id, c.id);
            assert!(p.can_enter(&c.id));
            assert!(p.choose_start("DEU").is_err());
            p.validate().unwrap();
        }
    }
    #[test]
    fn border_unlock_requires_complete_country_and_stage_replay_is_idempotent() {
        let mut p = WorldProgress::default();
        p.choose_start("VAT").unwrap();
        assert!(!p.can_enter("ITA"));
        assert!(p.record_clear(1).is_err());
        assert!(!p.record_clear(0).unwrap());
        assert!(!p.record_clear(0).unwrap());
        assert_eq!(p.completed("VAT"), 1);
        assert!(!p.can_enter("ITA"));
        assert!(!p.record_clear(1).unwrap());
        assert!(p.record_clear(2).unwrap());
        assert!(p.can_enter("ITA"));
        assert!(!p.can_enter("JPN"));
        assert!(!p.record_clear(2).unwrap());
        assert_eq!(p.cleared, vec!["VAT"]);
        p.enter("ITA").unwrap();
        assert_eq!(p.next_stage(), 0);
        assert_eq!(p.completed("VAT"), 3);
        p.validate().unwrap();
    }
    #[test]
    fn islands_are_reachable_via_symmetric_explicit_sea_routes() {
        let data = world();
        let mut visited = BTreeSet::from(["DEU".to_string()]);
        let mut todo = vec!["DEU".to_string()];
        while let Some(id) = todo.pop() {
            for neighbor in &data.country(&id).unwrap().neighbors {
                if visited.insert(neighbor.clone()) {
                    todo.push(neighbor.clone());
                }
            }
        }
        assert_eq!(visited.len(), data.countries.len());
        assert!(
            data.country("JPN")
                .unwrap()
                .sea_neighbors
                .contains(&"KOR".to_string())
        );
        for c in &data.countries {
            for n in &c.sea_neighbors {
                assert!(data.country(n).unwrap().sea_neighbors.contains(&c.id));
            }
        }
    }
    #[test]
    fn outbreak_reopens_final_stage_without_revoking_earned_routes() {
        let mut p = WorldProgress::default();
        p.choose_start("VAT").unwrap();
        for stage in 0..3 {
            p.record_clear(stage).unwrap();
        }
        assert!(p.can_enter("ITA"));
        assert!(p.reopen("VAT").unwrap());
        assert!(!p.cleared.contains(&"VAT".to_string()));
        assert_eq!(p.completed("VAT"), 2);
        assert!(p.can_enter("ITA"));
        assert!(!p.reopen("VAT").unwrap());
        assert!(p.record_clear(2).unwrap());
        p.validate().unwrap();
    }
    #[test]
    fn strategic_settings_are_validated_and_initial_map_is_selectable() {
        let mut data = world().clone();
        assert_eq!(data.strategy_config.parallel_unlock_freed, 2);
        assert_eq!(data.strategy_config.outbreak_interval_ticks, 36_000);
        let longest_boss = crate::content::content()
            .levels
            .iter()
            .map(|l| l.boss_time)
            .fold(0.0_f32, f32::max);
        assert!(
            data.strategy_config.outbreak_duration_ticks as f32 / 60.0 > longest_boss + 90.0,
            "A fresh deployment must have time to answer an outbreak"
        );
        assert!(WorldProgress::default().can_enter("NRU"));
        data.strategy_config.outbreak_chance_percent = 101;
        assert!(data.validate().is_err());
        let mut p = WorldProgress::default();
        p.enter("NRU").unwrap();
        p.validate().unwrap();
    }
    #[test]
    fn save_roundtrip_preserves_geographical_progress() {
        let mut p = WorldProgress::default();
        p.choose_start("MCO").unwrap();
        p.record_clear(0).unwrap();
        let text = serde_json::to_string(&p).unwrap();
        let restored: WorldProgress = serde_json::from_str(&text).unwrap();
        assert_eq!(p, restored);
        restored.validate().unwrap();
    }
    #[test]
    fn country_modifiers_are_authored_and_invalid_content_is_rejected() {
        let data = world();
        assert_ne!(
            data.country("EGY").unwrap().modifiers,
            data.country("FIN").unwrap().modifiers
        );
        let mut broken = data.clone();
        broken.countries[0].modifiers.enemy_health = f32::NAN;
        assert!(broken.validate().is_err());
        let mut broken = data.clone();
        broken.countries[0].neighbors.push("XXX".into());
        assert!(broken.validate().is_err());
    }
    #[test]
    fn custom_catalogues_reject_empty_disconnected_and_invalid_theme_data() {
        let mut data = world().clone();
        data.countries.clear();
        assert!(data.validate().is_err());
        let mut data = world().clone();
        for country in &mut data.countries {
            country.neighbors.clear();
            country.sea_neighbors.clear();
        }
        assert!(data.validate().is_err());
        let mut data = world().clone();
        data.countries[0].center[0] = f32::NAN;
        assert!(data.validate().is_err());
        let mut data = world().clone();
        data.countries[0].stage_theme_ids = vec!["missing-theme".into()];
        assert!(data.validate().is_err());
        let mut data = world().clone();
        data.countries[0].polygons[0] = vec![[0.0, 0.0]; 2049];
        assert!(data.validate().is_err());
    }
    #[test]
    fn local_dressing_is_stable_varied_and_uses_configured_tags() {
        let data = world();
        let country = data.country("DEU").unwrap();
        let first = country.stage_theme(0);
        assert_eq!(first, country.stage_theme(0));
        assert_ne!(first, country.stage_theme(1));
        assert_ne!(
            first.variation,
            data.country("FRA").unwrap().stage_theme(0).variation
        );
        let mut custom = data.clone();
        custom
            .countries
            .iter_mut()
            .find(|c| c.id == "DEU")
            .unwrap()
            .stage_theme_ids = vec!["waterfront".into()];
        custom.validate().unwrap();
        let local = custom.country("DEU").unwrap().stage_theme_with(7, &custom);
        assert_eq!(local.id, "waterfront");
        assert!(local.props.contains(&"dock_crane".to_string()));
    }
}
