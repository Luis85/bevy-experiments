//! Setting up a match from content: the [`MatchSetup`] of a scenario, then the map, the
//! players and the starting units.

use bevy_ecs::world::World;
use fl_content::{ContentDb, Faction, MapDef, ScenarioDef};
use fl_math::Angle;
use fl_protocol::{FactionId, PlayerId, SimId, TeamId};

use crate::build::{Content, MatchSetup};
use crate::map::MapInfo;
use crate::nav::{install_nav, nav_for};
use crate::players::{PlayerInfo, Players};
use crate::spatial::init_spatial;
use crate::spawn::{SpawnError, SpawnRequest, check_spawn, setup_spawn};
use crate::terrain::{init_terrain, terrain_of};

/// Why a map or scenario could not be set up.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SetupError {
    #[error("map '{0}' does not exist")]
    UnknownMap(String),
    #[error("scenario '{0}' does not exist")]
    UnknownScenario(String),
    #[error("faction '{0}' does not exist")]
    UnknownFaction(String),
    #[error("{0} player slots; at most 256 fit a PlayerId")]
    TooManyPlayers(usize),
    /// Starting unit `index` (in file order), of unit type `unit`, cannot spawn.
    #[error("starting unit {index} ('{unit}') cannot spawn: {error}")]
    Unit {
        index: usize,
        unit: String,
        error: SpawnError,
    },
    /// A unit outside a scenario cannot spawn (the scale scenario).
    #[error("cannot spawn a unit: {0}")]
    Spawn(#[from] SpawnError),
    /// The scale scenario's seed is not the match's: the sim would draw from one seed while
    /// the `MatchHeader` records another.
    #[error("the scale scenario's seed {scale} is not the match seed {matched}")]
    SeedMismatch { scale: u64, matched: u64 },
    /// The scale scenario's map `map` is not `map_m` metres square.
    #[error("the scale scenario's map '{map}' is not {map_m} m square")]
    MapSizeMismatch { map: String, map_m: i32 },
}

impl MatchSetup {
    /// The setup of scenario `scenario_id`: its seed, cheats off.
    ///
    /// # Errors
    /// [`SetupError::UnknownScenario`].
    pub fn for_scenario(content: &ContentDb, scenario_id: &str) -> Result<MatchSetup, SetupError> {
        let def = content
            .kind::<ScenarioDef>()
            .get(scenario_id)
            .ok_or_else(|| SetupError::UnknownScenario(scenario_id.to_owned()))?;
        Ok(MatchSetup {
            seed: def.seed,
            cheats_allowed: false,
        })
    }
}

/// Map `map_id` and its [`MapInfo`].
fn map_info<'c>(content: &'c ContentDb, map_id: &str) -> Result<(&'c MapDef, MapInfo), SetupError> {
    let map = content
        .kind::<MapDef>()
        .get(map_id)
        .ok_or_else(|| SetupError::UnknownMap(map_id.to_owned()))?;
    Ok((map, MapInfo::flat(map.size_m, map.cell_m)))
}

/// Insert the [`MapInfo`] of map `map_id`, its terrain ([`TerrainGrid`](crate::TerrainGrid))
/// and the navigation data built from it.
pub fn setup_map(world: &mut World, map_id: &str) -> Result<(), SetupError> {
    let content = world.resource::<Content>().0.clone();
    let (def, map) = map_info(&content, map_id)?;
    world.insert_resource(map);
    init_spatial(world);
    init_terrain(world, Some(def));
    Ok(())
}

/// Set up scenario `scenario_id`; see [`setup_scenario_def`].
pub fn setup_scenario(world: &mut World, scenario_id: &str) -> Result<Vec<SimId>, SetupError> {
    let def = world
        .resource::<Content>()
        .0
        .kind::<ScenarioDef>()
        .get(scenario_id)
        .cloned()
        .ok_or_else(|| SetupError::UnknownScenario(scenario_id.to_owned()))?;
    setup_scenario_def(world, &def)
}

/// Set up `def`: insert its map (with its terrain and navigation data) and [`Players`] (slot
/// `i` is `PlayerId(i)`), then spawn its units in file order through [`setup_spawn`]. Returns
/// their ids. The [`MatchSeed`](crate::MatchSeed) is left alone: it comes from the
/// [`MatchSetup`] ([`MatchSetup::for_scenario`] takes the scenario's seed).
///
/// All or nothing: the map, the players and every unit (type, player slot, position, a cell
/// its profile can stand on) are checked before anything changes.
///
/// # Errors
/// The first problem found; a unit's error ([`SetupError::Unit`]) names its index and type.
pub fn setup_scenario_def(world: &mut World, def: &ScenarioDef) -> Result<Vec<SimId>, SetupError> {
    let content = world.resource::<Content>().0.clone();
    let (map_def, map) = map_info(&content, &def.map)?;
    let terrain = terrain_of(Some(map_def), &map);
    let nav = nav_for(&content, &terrain, &map);
    let factions = content.kind::<Faction>();
    let mut players = Vec::with_capacity(def.players.len());
    for (slot, player) in def.players.iter().enumerate() {
        let id = u8::try_from(slot).map_err(|_| SetupError::TooManyPlayers(def.players.len()))?;
        let faction = factions
            .index_of(&player.faction)
            .ok_or_else(|| SetupError::UnknownFaction(player.faction.clone()))?;
        players.push(PlayerInfo {
            id: PlayerId(id),
            faction: FactionId(u16::try_from(faction).expect("fewer than 65536 factions")),
            team: TeamId(player.team),
            controller: player.controller.clone(),
        });
    }

    // Resolve and check every unit with setup_spawn's own checks, before the world changes.
    let players = Players { list: players };
    let mut requests = Vec::with_capacity(def.units.len());
    for (index, unit) in def.units.iter().enumerate() {
        let error = |error| SetupError::Unit {
            index,
            unit: unit.unit.clone(),
            error,
        };
        let request = SpawnRequest {
            unit: Some(
                content
                    .unit_id(&unit.unit)
                    .ok_or_else(|| error(SpawnError::UnknownUnit))?,
            ),
            owner: PlayerId(unit.player),
            pos: unit.pos,
            heading: Angle::from_degrees(unit.heading_deg),
            init: None,
        };
        check_spawn(&content, Some(&map), Some(&nav), &players, &request).map_err(error)?;
        requests.push((index, unit, request));
    }

    world.insert_resource(map);
    init_spatial(world);
    world.insert_resource(terrain);
    // The grid with its caches, exactly as a load rebuilds them (R53).
    install_nav(world, nav);
    world.insert_resource(players);
    requests
        .iter()
        .map(|(index, unit, request)| {
            setup_spawn(world, request).map_err(|error| SetupError::Unit {
                index: *index,
                unit: unit.unit.clone(),
                error,
            })
        })
        .collect()
}
