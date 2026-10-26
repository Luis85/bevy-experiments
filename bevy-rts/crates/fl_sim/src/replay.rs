//! Replays (spec §5.5, §15 B; ADR-0007, ADR-0021): a match's [`MatchHeader`], how it started,
//! every tick's commands and its state hashes, encoded with postcard, with named
//! [`SemanticCheck`]s next to the hashes and a journal summary.
//!
//! A [`ReplayRecorder`] records a match from the inputs and outputs of its ticks.
//! [`verify_replay`] plays a replay back on a fresh sim, through a [`ReplayDriver`], compares
//! every recorded hash and then runs the semantic checks; [`first_divergence`] finds where two
//! hash streams part. [`rebless_replay`] re-runs a replay on this build for `fl_cli rebless`.
//! [`Replay::to_json`] is the readable view (`fl_cli replay dump --json`).

use std::collections::BTreeMap;
use std::sync::Arc;

use bevy_app::App;
use bevy_ecs::world::World;
use fl_content::{ContentDb, TerrainDef};
use fl_math::FxVec2;
use fl_protocol::{HeaderMismatch, MatchHeader, SimId, Tick};
use fl_view::{TickDriver, TickInput, TickOutput};
use serde::{Deserialize, Serialize};

use crate::build::{Content, MatchHeaderRes, MatchSetup, RunConfig, build_sim_app};
use crate::commands::{remap_command_id, remap_command_ids};
use crate::feature::SimFeature;
use crate::ids::SimIndex;
use crate::nav::NavGrid;
use crate::runner::{ReplayDriver, SimRunner};
use crate::save::{LoadCtx, LoadError, load_world};
use crate::scale::ScaleScenario;
use crate::scenario::setup_scenario;
use crate::terrain::TerrainGrid;
use crate::units::Pose;

/// The version of the [`Replay`] encoding. [`Replay::from_bytes`] reads this one and format 3.
/// Version 2: [`ScaleScenario`] (in the scale start) gained the entity-mix fields.
/// Version 3: [`ScaleScenario`] gained `map` (the scale mix on terrain).
/// Version 4: the scale start is a versioned params blob ([`ReplayStart::Scale`]), and a
/// replay carries [`SemanticCheck`]s and a journal summary (ADR-0021, MCR-000).
pub const REPLAY_FORMAT_VERSION: u32 = 4;

/// The replay format version before [`REPLAY_FORMAT_VERSION`], which still reads.
const FORMAT_3: u32 = 3;

/// How a replayed match was set up before its first recorded tick.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ReplayStart {
    /// [`setup_scenario`] with this scenario id.
    Scenario(String),
    /// [`ScaleScenario::populate`] with the scale scenario that [`ScaleScenario::decode`] reads
    /// from `version` and `params` ([`ScaleScenario::encode`]): a new scale-mix field bumps
    /// [`ScaleScenario::VERSION`], not the replay format.
    Scale { version: u32, params: Vec<u8> },
    /// [`load_world`] of this save; the replay starts at its tick. The save's pending
    /// commands are not used: the replay's inputs deliver every command.
    Save(Vec<u8>),
}

impl ReplayStart {
    /// The start of a match set up by `scale` ([`ScaleScenario::encode`]).
    pub fn scale(scale: &ScaleScenario) -> ReplayStart {
        let (version, params) = scale.encode();
        ReplayStart::Scale { version, params }
    }
}

/// Something that happened to a match besides its commands.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ReplayEvent {
    /// The content was hot-reloaded at `tick`, to rules hash `rules_hash`: the replay cannot
    /// be played back with one content set.
    ContentReload { tick: Tick, rules_hash: u64 },
}

/// A named assertion about a replayed match, checked during every verify after the hashes
/// (ADR-0021). Checks follow from a scenario's setup (no unit dies, no unit stands on water),
/// never from a recorded run. A check whose `tick` the replay does not run fails.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum SemanticCheck {
    /// Unit `unit`'s `Pose.pos` is inside the rectangle `min..=max` after `tick` ran.
    UnitWithin {
        id: String,
        tick: Tick,
        unit: SimId,
        min: FxVec2,
        max: FxVec2,
    },
    /// Unit `unit` is alive after `tick` ran (or, with `exists: false`, it is not).
    UnitExists {
        id: String,
        tick: Tick,
        unit: SimId,
        exists: bool,
    },
    /// After `tick` ran, the number of entities ([`SimIndex`] length) is in `min..=max`.
    EntityCount {
        id: String,
        tick: Tick,
        min: u32,
        max: u32,
    },
    /// After every tick, no listed unit that is alive stands on a cell of terrain `terrain`
    /// (a content name, such as `"base.water"`). It fails at once if `terrain` is not a
    /// terrain of the content or the match has no terrain grid, where it could never fail.
    NeverOnTerrain {
        id: String,
        units: Vec<SimId>,
        terrain: String,
    },
}

impl SemanticCheck {
    /// The check's name, unique within its replay.
    pub fn id(&self) -> &str {
        match self {
            SemanticCheck::UnitWithin { id, .. }
            | SemanticCheck::UnitExists { id, .. }
            | SemanticCheck::EntityCount { id, .. }
            | SemanticCheck::NeverOnTerrain { id, .. } => id,
        }
    }

    /// The tick a one-shot check is taken after; `None` for a check of every tick.
    fn tick(&self) -> Option<Tick> {
        match self {
            SemanticCheck::UnitWithin { tick, .. }
            | SemanticCheck::UnitExists { tick, .. }
            | SemanticCheck::EntityCount { tick, .. } => Some(*tick),
            SemanticCheck::NeverOnTerrain { .. } => None,
        }
    }

    /// What is wrong with `world`, the state after `tick` ran; `None` if the check holds (or
    /// is not due at `tick`).
    fn failure(&self, tick: Tick, world: &World) -> Option<String> {
        if self.tick().is_some_and(|due| due != tick) {
            return None;
        }
        let index = world.resource::<SimIndex>();
        let pose = |unit: SimId| {
            index
                .get(unit)
                .and_then(|entity| world.get::<Pose>(entity))
                .copied()
        };
        match self {
            SemanticCheck::UnitWithin { unit, min, max, .. } => match pose(*unit) {
                None => Some(format!("{unit} is not alive or has no pose")),
                Some(Pose { pos, .. }) => {
                    (pos.x < min.x || pos.y < min.y || pos.x > max.x || pos.y > max.y).then(|| {
                        format!(
                            "{unit} at ({}, {}) is outside ({}, {})..=({}, {})",
                            pos.x, pos.y, min.x, min.y, max.x, max.y
                        )
                    })
                }
            },
            SemanticCheck::UnitExists { unit, exists, .. } => {
                let alive = index.get(*unit).is_some();
                (alive != *exists).then(|| {
                    if alive {
                        format!("{unit} is alive")
                    } else {
                        format!("{unit} is not alive")
                    }
                })
            }
            SemanticCheck::EntityCount { min, max, .. } => {
                let count = u32::try_from(index.len()).unwrap_or(u32::MAX);
                (count < *min || count > *max)
                    .then(|| format!("{count} entities, expected {min}..={max}"))
            }
            SemanticCheck::NeverOnTerrain { units, terrain, .. } => {
                let known = world
                    .get_resource::<Content>()
                    .is_some_and(|content| content.0.kind::<TerrainDef>().get(terrain).is_some());
                if !known {
                    return Some(format!(
                        "{terrain} is not a terrain of the content: the check could never fail"
                    ));
                }
                let (Some(nav), Some(grid)) = (
                    world.get_resource::<NavGrid>(),
                    world.get_resource::<TerrainGrid>(),
                ) else {
                    return Some(
                        "the match has no terrain grid (NavGrid and TerrainGrid): the check could \
                         never fail"
                            .to_owned(),
                    );
                };
                units.iter().find_map(|&unit| {
                    let (x, y) = nav.cell_of(pose(unit)?.pos);
                    (grid.terrain_at(x, y) == Some(terrain.as_str()))
                        .then(|| format!("{unit} stands on {terrain} at cell ({x}, {y})"))
                })
            }
        }
    }
}

/// A [`SemanticCheck`] that did not hold: its id, the tick after which it failed, and what was
/// wrong.
#[derive(Clone, Debug, PartialEq)]
pub struct SemanticFailure {
    pub id: String,
    pub tick: Tick,
    pub detail: String,
}

/// A recorded match.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Replay {
    /// [`REPLAY_FORMAT_VERSION`]; first, so a reader can check it before decoding the rest.
    pub version: u32,
    pub header: MatchHeader,
    pub start: ReplayStart,
    /// The state hash interval the match was recorded with.
    pub hash_interval: u32,
    /// The number of ticks recorded.
    pub ticks: u32,
    /// The input of every tick that had commands, in tick order.
    pub inputs: Vec<TickInput>,
    /// Every state hash taken, as `(tick, hash)`.
    pub hashes: Vec<(Tick, u64)>,
    pub events: Vec<ReplayEvent>,
    /// The checks every verify runs after the hashes (ADR-0021).
    pub semantic: Vec<SemanticCheck>,
    /// The number of journal records of each kind over the recorded ticks (empty when the
    /// journal was off), for re-bless reports. Observe-only: verification never reads it.
    pub journal_summary: BTreeMap<String, u32>,
}

/// The format-3 replay, frozen as it was at tag `m1` (ADR-0021): read by
/// [`Replay::from_bytes`] and converted to the current format. [`MatchHeader`], [`TickInput`]
/// and [`ReplayEvent`] are shared with format 4; a change to their encoding needs its own
/// frozen copy here.
mod v3 {
    use fl_protocol::{MatchHeader, Tick};
    use fl_view::TickInput;
    use serde::{Deserialize, Serialize};

    use super::ReplayEvent;

    /// `crate::ScaleScenario` at tag `m1`.
    #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
    pub(super) struct ScaleScenario {
        pub units: u32,
        pub players: u8,
        pub map_m: i32,
        pub wander_m: i32,
        pub seed: u64,
        pub followers_per_unit: u32,
        pub projectiles: u32,
        pub projectile_ttl: u32,
        pub sensor_every: u32,
        pub map: Option<String>,
    }

    #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
    pub(super) enum ReplayStart {
        Scenario(String),
        Scale(ScaleScenario),
        Save(Vec<u8>),
    }

    #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
    pub(super) struct Replay {
        pub version: u32,
        pub header: MatchHeader,
        pub start: ReplayStart,
        pub hash_interval: u32,
        pub ticks: u32,
        pub inputs: Vec<TickInput>,
        pub hashes: Vec<(Tick, u64)>,
        pub events: Vec<ReplayEvent>,
    }
}

impl From<v3::Replay> for Replay {
    /// The format-3 replay in the current format: the scale scenario as version-1 params, no
    /// semantic checks, an empty journal summary.
    fn from(old: v3::Replay) -> Replay {
        let start = match old.start {
            v3::ReplayStart::Scenario(id) => ReplayStart::Scenario(id),
            v3::ReplayStart::Save(bytes) => ReplayStart::Save(bytes),
            v3::ReplayStart::Scale(s) => ReplayStart::scale(&ScaleScenario {
                units: s.units,
                players: s.players,
                map_m: s.map_m,
                wander_m: s.wander_m,
                seed: s.seed,
                followers_per_unit: s.followers_per_unit,
                projectiles: s.projectiles,
                projectile_ttl: s.projectile_ttl,
                sensor_every: s.sensor_every,
                map: s.map,
            }),
        };
        Replay {
            version: REPLAY_FORMAT_VERSION,
            header: old.header,
            start,
            hash_interval: old.hash_interval,
            ticks: old.ticks,
            inputs: old.inputs,
            hashes: old.hashes,
            events: old.events,
            semantic: Vec::new(),
            journal_summary: BTreeMap::new(),
        }
    }
}

/// `T` decoded from all of `bytes`.
fn decode_all<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> Result<T, ReplayError> {
    match postcard::take_from_bytes::<T>(bytes) {
        Ok((value, [])) => Ok(value),
        Ok((_, rest)) => Err(ReplayError::Decode(format!(
            "{} trailing bytes after the replay",
            rest.len()
        ))),
        Err(err) => Err(ReplayError::Decode(err.to_string())),
    }
}

impl Replay {
    /// The postcard encoding (format [`REPLAY_FORMAT_VERSION`]).
    ///
    /// # Panics
    /// Never: every field serializes.
    pub fn to_bytes(&self) -> Vec<u8> {
        postcard::to_allocvec(self).expect("replays serialize")
    }

    /// Decode [`Replay::to_bytes`]'s output, or a format-3 replay (frozen `mod v3`), which
    /// reads as the current format with no semantic checks and an empty journal summary.
    ///
    /// # Errors
    /// [`ReplayError::Version`] if the bytes are another format version,
    /// [`ReplayError::Decode`] if they do not decode or have trailing bytes.
    pub fn from_bytes(bytes: &[u8]) -> Result<Replay, ReplayError> {
        let (found, _) = postcard::take_from_bytes::<u32>(bytes)
            .map_err(|err| ReplayError::Decode(err.to_string()))?;
        match found {
            REPLAY_FORMAT_VERSION => decode_all::<Replay>(bytes),
            FORMAT_3 => decode_all::<v3::Replay>(bytes).map(Replay::from),
            _ => Err(ReplayError::Version { found }),
        }
    }

    /// This replay with the semantic checks `checks` instead of its own.
    #[must_use]
    pub fn with_semantics(mut self, checks: Vec<SemanticCheck>) -> Replay {
        self.semantic = checks;
        self
    }

    /// The whole replay as JSON, the readable debug view: command payloads stay postcard
    /// bytes, a [`ReplayStart::Save`] stays the save's bytes and a scale start its params.
    ///
    /// # Panics
    /// Never: every field serializes to JSON.
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::to_value(self).expect("replays serialize to JSON")
    }

    /// Whether the replay can be played back: false if the content was reloaded during the
    /// match.
    pub fn reproducible(&self) -> bool {
        !self
            .events
            .iter()
            .any(|event| matches!(event, ReplayEvent::ContentReload { .. }))
    }
}

/// Records a [`Replay`] from a match's tick inputs and outputs.
#[derive(Clone, Debug)]
pub struct ReplayRecorder {
    replay: Replay,
}

impl ReplayRecorder {
    /// A recording of the match with `header`, set up by `start`, hashing every
    /// `hash_interval` ticks; it has no semantic checks ([`Replay::with_semantics`] adds them).
    pub fn new(header: MatchHeader, start: ReplayStart, hash_interval: u32) -> Self {
        ReplayRecorder {
            replay: Replay {
                version: REPLAY_FORMAT_VERSION,
                header,
                start,
                hash_interval,
                ticks: 0,
                inputs: Vec::new(),
                hashes: Vec::new(),
                events: Vec::new(),
                semantic: Vec::new(),
                journal_summary: BTreeMap::new(),
            },
        }
    }

    /// Record one tick: its input if it had commands, its hash if one was taken, and its
    /// journal records' kinds in the summary.
    pub fn record(&mut self, input: &TickInput, output: &TickOutput) {
        debug_assert_eq!(input.tick, output.tick, "an input and its own output");
        self.replay.ticks += 1;
        if !input.commands.is_empty() {
            self.replay.inputs.push(input.clone());
        }
        if let Some(hash) = output.hash {
            self.replay.hashes.push((output.tick, hash));
        }
        summarize(&mut self.replay.journal_summary, output);
    }

    /// The replay recorded so far.
    pub fn finish(self) -> Replay {
        self.replay
    }
}

/// Count `output`'s journal records by kind into `summary`.
fn summarize(summary: &mut BTreeMap<String, u32>, output: &TickOutput) {
    for entry in &output.journal {
        let count = match summary.get_mut(&entry.kind) {
            Some(count) => count,
            None => summary.entry(entry.kind.clone()).or_default(),
        };
        *count = count.saturating_add(1);
    }
}

/// What playing a replay back found.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq)]
pub enum VerifyOutcome {
    /// Every recorded hash matched over `ticks` ticks and every semantic check held;
    /// `warnings` are the header's (another build id).
    Verified { ticks: u32, warnings: Vec<String> },
    /// A recorded hash did not match: the earliest such tick. Hashes are checked before the
    /// semantic checks.
    Diverged(Divergence),
    /// Every hash matched, but these semantic checks failed (each at its first failing tick),
    /// in the replay's order.
    SemanticFailed(Vec<SemanticFailure>),
    /// The replay has a [`ReplayEvent::ContentReload`]; it was not played back.
    NonReproducible,
}

/// The first tick at which two hash streams differ.
#[derive(Debug, Clone, PartialEq)]
pub struct Divergence {
    pub tick: Tick,
    /// The hash in the reference stream (the replay's).
    pub expected: u64,
    /// The hash in the other stream (the playback's).
    pub actual: u64,
}

/// What [`rebless_replay`] found re-running a replay on this build, for the re-bless report.
#[derive(Clone, Debug, PartialEq)]
pub struct ReblessEntry {
    /// The first tick at which the recorded and the new hash streams differ; `None`: equal.
    pub first_divergence: Option<Divergence>,
    /// `(kind, recorded count, new count)` for every journal record kind whose count differs
    /// between the recorded and the new journal summary, by kind.
    pub journal_diff: Vec<(String, u32, u32)>,
    /// The number of entities ([`SimIndex`] length) before the first tick and after the last.
    pub entities: (u32, u32),
    /// The semantic checks that failed on this build.
    pub semantic: Vec<SemanticFailure>,
}

/// Why a replay could not be read or played back.
#[derive(Debug, thiserror::Error)]
pub enum ReplayError {
    #[error("the replay does not decode: {0}")]
    Decode(String),
    #[error(
        "replay format version {found} is not supported (this build reads {} and {})",
        FORMAT_3,
        REPLAY_FORMAT_VERSION
    )]
    Version { found: u32 },
    #[error(transparent)]
    Header(HeaderMismatch),
    #[error("the replay's match cannot be set up: {0}")]
    Setup(String),
    #[error("a recorded command cannot be mapped to this build: {0}")]
    Command(String),
}

/// The sim of `r`, set up per its start: [`verify_replay`]'s steps 2 and 3, shared with the
/// `fl_cli` agent session (inspect, bisect). Build the sim (or load the save) with the
/// recorded seed and cheats flag and the caller's `run` config, check the recorded header with
/// [`MatchHeader::check_replay`](fl_protocol::MatchHeader::check_replay), check that every
/// recorded command's type maps by name to this build ([`replay_inputs`]), then set up the
/// [`ReplayStart`]. Returns the sim and the check's warnings (a different build id). Play it
/// back with the inputs of [`replay_inputs`], never `r.inputs` as they are.
///
/// # Errors
/// [`ReplayError::Header`] if the recorded header does not match this build,
/// [`ReplayError::Command`] if a recorded command type is not in this build, or
/// [`ReplayError::Setup`] if the sim does not build, the save does not load, the scale
/// params do not decode or the start does not set up.
pub fn replay_app(
    r: &Replay,
    features: &[Arc<dyn SimFeature>],
    content: Arc<ContentDb>,
    run: RunConfig,
) -> Result<(App, Vec<String>), ReplayError> {
    let scale = match &r.start {
        ReplayStart::Scale { version, params } => {
            Some(ScaleScenario::decode(*version, params).map_err(ReplayError::Setup)?)
        }
        ReplayStart::Scenario(_) | ReplayStart::Save(_) => None,
    };
    let mut app = match &r.start {
        ReplayStart::Save(bytes) => match load_world(features, content, bytes, run) {
            Ok(loaded) => loaded.app,
            Err(LoadError::Header(mismatch)) => return Err(ReplayError::Header(mismatch)),
            Err(err) => return Err(ReplayError::Setup(err.to_string())),
        },
        ReplayStart::Scenario(_) | ReplayStart::Scale { .. } => {
            let setup = MatchSetup {
                seed: r.header.seed,
                cheats_allowed: r.header.cheats_allowed,
            };
            build_sim_app(features, content, setup, run)
                .map_err(|err| ReplayError::Setup(err.to_string()))?
        }
    };
    let warnings = app
        .world()
        .resource::<MatchHeaderRes>()
        .0
        .check_replay(&r.header)
        .map_err(ReplayError::Header)?;
    {
        let current = &app.world().resource::<MatchHeaderRes>().0;
        let ctx = LoadCtx::new(&r.header.string_tables, &current.string_tables);
        for cmd in r.inputs.iter().flat_map(|input| &input.commands) {
            remap_command_id(&ctx, cmd.kind.type_id).map_err(ReplayError::Command)?;
        }
    }
    let world = app.world_mut();
    match (&r.start, scale) {
        (ReplayStart::Scenario(id), _) => {
            setup_scenario(world, id).map_err(|err| ReplayError::Setup(err.to_string()))?;
        }
        (ReplayStart::Scale { .. }, Some(scale)) => {
            scale
                .populate(world)
                .map_err(|err| ReplayError::Setup(err.to_string()))?;
        }
        _ => {}
    }
    Ok((app, warnings))
}

/// `r`'s inputs for the sim in `app` (built by [`replay_app`]): every command's type id
/// mapped by name from the recorded header's `"commands"` string table to the sim's. Command
/// type ids are positions in name order, so a build that adds a command name sorting earlier
/// moves them.
///
/// # Errors
/// [`ReplayError::Command`] if a recorded command type is not in the sim.
///
/// # Panics
/// If `app` was not built by [`build_sim_app`].
pub fn replay_inputs(r: &Replay, app: &App) -> Result<Vec<TickInput>, ReplayError> {
    let current = &app.world().resource::<MatchHeaderRes>().0;
    let ctx = LoadCtx::new(&r.header.string_tables, &current.string_tables);
    r.inputs
        .iter()
        .map(|input| {
            let mut input = input.clone();
            remap_command_ids(&ctx, &mut input.commands).map_err(ReplayError::Command)?;
            Ok(input)
        })
        .collect()
}

/// What one playback of a replay produced.
struct Playback {
    /// This build's header of the replayed sim.
    header: MatchHeader,
    /// The header check's warnings.
    warnings: Vec<String>,
    /// The inputs played, in this build's command ids.
    inputs: Vec<TickInput>,
    hashes: Vec<(Tick, u64)>,
    journal_summary: BTreeMap<String, u32>,
    semantic: Vec<SemanticFailure>,
    /// Entities before the first tick and after the last.
    entities: (u32, u32),
}

/// The number of entities of `world`.
fn entity_count(world: &World) -> u32 {
    u32::try_from(world.resource::<SimIndex>().len()).unwrap_or(u32::MAX)
}

/// Set `r` up with `run` ([`replay_app`]), run its `r.ticks` ticks through a [`ReplayDriver`]
/// over [`replay_inputs`], and check its semantic checks after every tick: each check fails at
/// most once, at its first failing tick, and a one-shot check whose tick did not run fails.
fn play_back(
    r: &Replay,
    features: &[Arc<dyn SimFeature>],
    content: Arc<ContentDb>,
    run: RunConfig,
) -> Result<Playback, ReplayError> {
    let (app, warnings) = replay_app(r, features, content, run)?;
    let inputs = replay_inputs(r, &app)?;
    let header = app.world().resource::<MatchHeaderRes>().0.clone();
    let before = entity_count(app.world());
    let mut runner = SimRunner::new(app);
    let first = runner.next_tick();
    let mut driver = ReplayDriver::new(inputs.clone(), first, None);
    let mut hashes = Vec::new();
    let mut journal_summary = BTreeMap::new();
    let mut failed: Vec<Option<SemanticFailure>> = vec![None; r.semantic.len()];
    for _ in 0..r.ticks {
        let input = driver
            .poll()
            .expect("a replay driver without an end always ticks");
        let output = runner.step(input);
        if let Some(hash) = output.hash {
            hashes.push((output.tick, hash));
        }
        summarize(&mut journal_summary, &output);
        for (check, failure) in r.semantic.iter().zip(&mut failed) {
            if failure.is_none()
                && let Some(detail) = check.failure(output.tick, runner.world())
            {
                *failure = Some(SemanticFailure {
                    id: check.id().to_owned(),
                    tick: output.tick,
                    detail,
                });
            }
        }
    }
    let end = Tick(first.0 + r.ticks);
    let semantic = r
        .semantic
        .iter()
        .zip(failed)
        .filter_map(|(check, failure)| {
            failure.or_else(|| {
                let tick = check.tick()?;
                (tick < first || tick >= end).then(|| SemanticFailure {
                    id: check.id().to_owned(),
                    tick,
                    detail: format!(
                        "tick {} is not in the replay (ticks {}..{})",
                        tick.0, first.0, end.0
                    ),
                })
            })
        })
        .collect();
    Ok(Playback {
        header,
        warnings,
        inputs,
        hashes,
        journal_summary,
        semantic,
        entities: (before, entity_count(runner.world())),
    })
}

/// Play `r` back on a fresh sim of `features` and `content` and compare its hashes, then run
/// its semantic checks:
/// 1. a replay of another format version is [`ReplayError::Version`]; a non-reproducible one
///    is [`VerifyOutcome::NonReproducible`];
/// 2. build the sim with the recorded seed, cheats flag and hash interval (journal off), and
///    check the recorded header with
///    [`MatchHeader::check_replay`](fl_protocol::MatchHeader::check_replay);
/// 3. set up the [`ReplayStart`];
/// 4. run `r.ticks` ticks through a [`ReplayDriver`] over [`replay_inputs`], checking the
///    [`SemanticCheck`]s after every tick;
/// 5. [`first_divergence`] of the recorded and the replayed hashes
///    ([`VerifyOutcome::Diverged`]), then the semantic failures
///    ([`VerifyOutcome::SemanticFailed`]).
///
/// # Errors
/// [`ReplayError::Version`], [`ReplayError::Header`], [`ReplayError::Command`], or
/// [`ReplayError::Setup`] if the replay
/// has ticks but no hashes (nothing to verify), the sim does not build, the start does not set
/// up, or the playback took no hash at a recorded hash's tick.
pub fn verify_replay(
    r: &Replay,
    features: &[Arc<dyn SimFeature>],
    content: Arc<ContentDb>,
) -> Result<VerifyOutcome, ReplayError> {
    if r.version != REPLAY_FORMAT_VERSION {
        return Err(ReplayError::Version { found: r.version });
    }
    if !r.reproducible() {
        return Ok(VerifyOutcome::NonReproducible);
    }
    if r.ticks > 0 && r.hashes.is_empty() {
        return Err(ReplayError::Setup(format!(
            "the replay has {} ticks but no hashes: nothing to verify",
            r.ticks
        )));
    }
    let run = RunConfig {
        hash_interval: Some(r.hash_interval),
        journal: Some(false),
        ..RunConfig::default()
    };
    let played = play_back(r, features, content, run)?;
    let taken: BTreeMap<Tick, u64> = played.hashes.iter().copied().collect();
    if let Some(&(tick, _)) = r.hashes.iter().find(|(tick, _)| !taken.contains_key(tick)) {
        return Err(ReplayError::Setup(format!(
            "the replay has a hash at tick {}, which the playback did not take",
            tick.0
        )));
    }
    Ok(match first_divergence(&r.hashes, &played.hashes) {
        Some(divergence) => VerifyOutcome::Diverged(divergence),
        None if !played.semantic.is_empty() => VerifyOutcome::SemanticFailed(played.semantic),
        None => VerifyOutcome::Verified {
            ticks: r.ticks,
            warnings: played.warnings,
        },
    })
}

/// Re-run `r` on this build for a re-bless (`fl_cli rebless`, ADR-0021): set up its start and
/// play its remapped inputs with the journal on and the recorded hash interval, checking its
/// semantic checks. The new replay has this build's header, the new hashes and journal summary,
/// and the same start, inputs (in this build's command ids), ticks, events and semantic checks;
/// the entry compares it with `r`.
///
/// # Errors
/// [`ReplayError::Version`] for a replay of another format version, [`ReplayError::Setup`]
/// for a non-reproducible one, and any error of [`replay_app`] or [`replay_inputs`].
pub fn rebless_replay(
    r: &Replay,
    features: &[Arc<dyn SimFeature>],
    content: Arc<ContentDb>,
) -> Result<(Replay, ReblessEntry), ReplayError> {
    if r.version != REPLAY_FORMAT_VERSION {
        return Err(ReplayError::Version { found: r.version });
    }
    if !r.reproducible() {
        return Err(ReplayError::Setup(
            "the replay is not reproducible: the content was reloaded during the match".to_owned(),
        ));
    }
    let run = RunConfig {
        hash_interval: Some(r.hash_interval),
        journal: Some(true),
        ..RunConfig::default()
    };
    let played = play_back(r, features, content, run)?;
    let mut kinds: Vec<&String> = r
        .journal_summary
        .keys()
        .chain(played.journal_summary.keys())
        .collect();
    kinds.sort();
    kinds.dedup();
    let journal_diff = kinds
        .into_iter()
        .filter_map(|kind| {
            let before = r.journal_summary.get(kind).copied().unwrap_or(0);
            let after = played.journal_summary.get(kind).copied().unwrap_or(0);
            (before != after).then(|| (kind.clone(), before, after))
        })
        .collect();
    let entry = ReblessEntry {
        first_divergence: first_divergence(&r.hashes, &played.hashes),
        journal_diff,
        entities: played.entities,
        semantic: played.semantic,
    };
    let replay = Replay {
        version: REPLAY_FORMAT_VERSION,
        header: played.header,
        start: r.start.clone(),
        hash_interval: r.hash_interval,
        ticks: r.ticks,
        inputs: played.inputs,
        hashes: played.hashes,
        events: r.events.clone(),
        semantic: r.semantic.clone(),
        journal_summary: played.journal_summary,
    };
    Ok((replay, entry))
}

/// The earliest tick that both `a` (the reference) and `b` hashed, with different hashes.
/// Ticks only one of them hashed are not compared.
pub fn first_divergence(a: &[(Tick, u64)], b: &[(Tick, u64)]) -> Option<Divergence> {
    let b: BTreeMap<Tick, u64> = b.iter().copied().collect();
    a.iter()
        .filter_map(|&(tick, expected)| {
            let actual = *b.get(&tick)?;
            (actual != expected).then_some(Divergence {
                tick,
                expected,
                actual,
            })
        })
        .min_by_key(|divergence| divergence.tick)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A format-3 replay with a scale start reads as format 4 with the same scale scenario as
    /// version-1 params.
    #[test]
    fn format_3_scale_start_reads_as_versioned_params() {
        let old_scale = v3::ScaleScenario {
            units: 40,
            players: 2,
            map_m: 400,
            wander_m: 30,
            seed: 9,
            followers_per_unit: 2,
            projectiles: 12,
            projectile_ttl: 4,
            sensor_every: 5,
            map: Some("base.scale_2km".to_owned()),
        };
        let old = v3::Replay {
            version: FORMAT_3,
            header: MatchHeader {
                format_version: 1,
                build_id: "test".to_owned(),
                sim_version: 2,
                features: vec![("core".to_owned(), 2)],
                mods: Vec::new(),
                rules_hash: 1,
                cheats_allowed: false,
                seed: 9,
                string_tables: fl_protocol::StringTables::default(),
            },
            start: v3::ReplayStart::Scale(old_scale),
            hash_interval: 5,
            ticks: 3,
            inputs: Vec::new(),
            hashes: vec![(Tick(0), 7), (Tick(1), 8), (Tick(2), 9)],
            events: Vec::new(),
        };
        let bytes = postcard::to_allocvec(&old).expect("format 3 encodes");
        let replay = Replay::from_bytes(&bytes).expect("format 3 reads");
        let expected = ScaleScenario {
            units: 40,
            players: 2,
            map_m: 400,
            wander_m: 30,
            seed: 9,
            followers_per_unit: 2,
            projectiles: 12,
            projectile_ttl: 4,
            sensor_every: 5,
            map: Some("base.scale_2km".to_owned()),
        };
        assert_eq!(replay.start, ReplayStart::scale(&expected));
        let ReplayStart::Scale { version, params } = &replay.start else {
            panic!("a scale start");
        };
        assert_eq!(ScaleScenario::decode(*version, params), Ok(expected));
        assert_eq!(replay.version, REPLAY_FORMAT_VERSION);
        assert_eq!(replay.hashes, old.hashes);
        assert!(replay.semantic.is_empty() && replay.journal_summary.is_empty());
    }
}
