//! Building scenario apps, loading command files and driving a run tick by
//! tick (`run`, `inspect`, `journal`, `replay record`, `diverge`).
//!
//! **Which content builds a scenario** ([`CONTENTS`], the factory
//! registry): a `--scenario` name is offered to each content in turn; the
//! village is last and takes everything else (`village`, `village_n`,
//! `village_nav`, and the village's own error for unknown names), so a run
//! without `--scenario` is the village exactly as before. The testbed takes
//! `testbed` and `testbed:<name>` (an embedded scenario file; a bare
//! embedded name such as `smithy` is qualified to `testbed:smithy`). A
//! `--scenario` that names an existing file, ends in `.json` or contains a
//! path separator is a scenario **file** (`{"version":1,"content":..}`), built by the content's
//! `npc_agent_io` scenario loader. Replays carry the scenario name, so
//! `replay verify` / `diverge` find the content again; a file scenario is
//! recorded as a session record (the scenario document + its replay).

use std::collections::{BTreeMap, VecDeque};
use std::path::{Path, PathBuf};

use bevy_app::App;
use bevy_ecs::prelude::*;
use npc_framework::agent_io::scenario::{SESSION_VERSION, ScenarioDoc, SessionRecord, load};
use npc_framework::ai::BrainStats;
use npc_framework::kernel::commands::{CommandLog, SkippedCommands, submit_deferred};
use npc_framework::kernel::journal::{set_journal, with_journal_from_build};
use npc_framework::kernel::replay::{AppFactory, Replay, ScenarioInfo, VerifyReport};
use npc_framework::prelude::{
    CommandError, NeedRegistry, Needs, SimClock, SimConfig, SimId, run_ticks,
};
use npc_framework::tools::commands_file::TimedCommand;
use npc_framework::tools::new_app_journaled;
use npc_framework::tools::output::Failure;
use npc_framework::tools::recorder::Recorder;
use serde::Serialize;
use serde_json::Value;
use village_content::needs::VillagerNeeds;
use village_content::{Activity, Villager};

/// A content npcctl can build named scenarios of (the factory registry).
pub struct ContentEntry {
    /// Content name (`"village"`, `"testbed"`).
    pub name: &'static str,
    /// Builds a scenario by name (the replay `AppFactory`).
    pub factory: AppFactory,
    /// Whether a scenario name is this content's.
    pub claims: fn(&str) -> bool,
    /// Whether `--villagers` sizes the scenario (`bench` sizes).
    pub sized: bool,
}

fn claims_all(_: &str) -> bool {
    true
}

/// Every content, in the order names are offered; the village (last) takes
/// any name the others do not claim.
pub const CONTENTS: [ContentEntry; 2] = [
    ContentEntry {
        name: "testbed",
        factory: testbed_content::replay::try_new_replay_app,
        claims: testbed_content::replay::is_testbed,
        sized: false,
    },
    ContentEntry {
        name: "village",
        factory: village_content::try_new_sim_app,
        claims: claims_all,
        sized: true,
    },
];

/// The content of scenario `name`.
pub fn content_of(name: &str) -> &'static ContentEntry {
    CONTENTS
        .iter()
        .find(|c| (c.claims)(name))
        .expect("the village claims every name")
}

/// Whether `--scenario` names a scenario file rather than a scenario: an
/// existing file wins over a name (the lookup order of `serve` and the
/// windowed testbed), and anything that looks like a path (`*.json`, a
/// separator) is a file even when missing (so the error says so).
fn is_file_arg(s: &str) -> bool {
    Path::new(s).is_file() || s.ends_with(".json") || s.contains('/') || s.contains('\\')
}

/// The scenario loaders npcctl knows (`serve`, scenario files).
pub fn loaders() -> npc_framework::agent_io::ScenarioLoaders {
    crate::agent::loaders()
}

/// Where a scenario comes from.
#[derive(Clone, Debug)]
pub enum Source {
    /// A named scenario of a registered content.
    Named(&'static ContentEntry),
    /// A scenario file, built by its content's scenario loader.
    File {
        path: PathBuf,
        doc: Box<ScenarioDoc>,
    },
}

impl std::fmt::Debug for ContentEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name)
    }
}

/// Which world to build.
#[derive(Clone, Debug)]
pub struct Scenario {
    pub seed: u64,
    /// The scenario name (`"village"`, `"village_n"`, `"testbed"`,
    /// `"testbed:smithy"`), or a scenario file's path.
    pub name: String,
    pub villagers: u32,
    pub source: Source,
}

impl Scenario {
    /// `scenario` defaults to `"village"` for one villager, else
    /// `"village_n"`; `seed` to 1 (a scenario file: its own seed, else 1).
    pub fn new(seed: Option<u64>, scenario: Option<&str>, villagers: u32) -> Result<Self, Failure> {
        let name = scenario.unwrap_or(if villagers == 1 {
            "village"
        } else {
            "village_n"
        });
        if is_file_arg(name) {
            let path = PathBuf::from(name);
            let doc = read_scenario_file(&path)?;
            return Ok(Self {
                seed: seed.or(doc.seed).unwrap_or(1),
                name: name.to_owned(),
                villagers,
                source: Source::File {
                    path,
                    doc: Box::new(doc),
                },
            });
        }
        // A bare embedded testbed scenario (`smithy`) is `testbed:smithy`.
        let name = if !testbed_content::replay::is_testbed(name)
            && !name.starts_with("village")
            && testbed_content::scenario::scenario_text(name).is_some()
        {
            testbed_content::replay::qualified(name)
        } else {
            name.to_owned()
        };
        Ok(Self {
            seed: seed.unwrap_or(1),
            source: Source::Named(content_of(&name)),
            name,
            villagers,
        })
    }

    /// The village (the scenario npcctl always had: JSON shapes and replay
    /// headers exactly as before).
    pub fn is_village(&self) -> bool {
        matches!(self.source, Source::Named(c) if c.name == "village")
    }

    /// Content name (`"village"`, `"testbed"`), a file's `content`.
    pub fn content(&self) -> &str {
        match &self.source {
            Source::Named(c) => c.name,
            Source::File { doc, .. } => &doc.content,
        }
    }

    pub fn build(&self) -> Result<App, Failure> {
        match &self.source {
            Source::Named(c) => (c.factory)(self.seed, &self.name, self.villagers),
            Source::File { doc, .. } => load(&loaders(), doc, Some(self.seed)).map(|l| l.app),
        }
        .map_err(Failure::usage)
    }

    /// [`Scenario::build`] with the journal on from the start, so it also
    /// has the scenario's tick-0 `Spawn` entries (`journal` by default,
    /// `inspect --journal-from-build true`).
    pub fn build_journaled(&self, verbose: bool) -> Result<App, Failure> {
        match &self.source {
            Source::Named(c) => {
                new_app_journaled(c.factory, self.seed, &self.name, self.villagers, verbose)
            }
            Source::File { doc, .. } => {
                with_journal_from_build(verbose, || load(&loaders(), doc, Some(self.seed))).map(
                    |l| {
                        let mut app = l.app;
                        set_journal(app.world_mut(), true, verbose);
                        app
                    },
                )
            }
        }
        .map_err(Failure::usage)
    }

    /// The factory of a named scenario (`diverge`, `bench`).
    pub fn factory(&self, what: &str) -> Result<AppFactory, Failure> {
        match &self.source {
            Source::Named(c) => Ok(c.factory),
            Source::File { path, .. } => Err(Failure::usage(format!(
                "{what} needs a named scenario, not the file {} (record it with \
                 `replay record --scenario FILE`, which writes a session record)",
                path.display()
            ))),
        }
    }

    /// A replay of this scenario for `ticks` ticks with `commands`.
    pub fn replay(&self, ticks: u64, commands: &[TimedCommand]) -> Replay {
        let mut replay = Replay::new(self.seed, &self.name, self.villagers, ticks);
        replay.commands = commands
            .iter()
            .map(|c| (c.tick, c.seq, c.cmd.clone()))
            .collect();
        replay
    }
}

fn read_scenario_file(path: &Path) -> Result<ScenarioDoc, Failure> {
    let text = std::fs::read_to_string(path).map_err(|e| {
        Failure::usage(format!("cannot read scenario {}: {e}", path.display()))
            .with("path", path.display().to_string())
    })?;
    ScenarioDoc::parse(&text).map_err(|e| {
        Failure::usage(format!("scenario {}: {e}", path.display()))
            .with("path", path.display().to_string())
    })
}

/// A recorded run: a replay (named scenarios) or a session record (a
/// scenario file: the document travels with the replay).
#[derive(Clone, Debug, Serialize)]
#[serde(untagged)]
pub enum Recorded {
    Replay(Replay),
    Session(SessionRecord),
}

/// Verifies a replay with the factory of its scenario's content.
pub fn verify_replay(replay: &Replay) -> VerifyReport {
    npc_framework::kernel::replay::verify(replay, content_of(&replay.scenario).factory)
}

/// Verifies a replay or a session record file's JSON: a session record
/// (`{"scenario": {..}, "replay": {..}}`) is re-run from its scenario
/// document, a replay by its scenario name.
pub fn is_session_record(v: &Value) -> bool {
    v.get("replay").is_some_and(Value::is_object) && v.get("scenario").is_some_and(Value::is_object)
}

/// Drives an app one tick at a time, queueing each command right before its
/// tick runs with [`submit_deferred`]: a command is validated when it
/// applies, after the earlier commands of its tick, so it may target a unit
/// that one of them spawns. A command that does not validate then fails the
/// run (exit 2, with its file index and the `CommandError`), as a rejected
/// submission did before.
pub struct Driver {
    pub app: App,
    pending: VecDeque<TimedCommand>,
    /// Commands queued for the tick being run, by `(tick, seq)`.
    queued: BTreeMap<(u64, u32), TimedCommand>,
    /// `SkippedCommands` entries already checked.
    skipped_seen: usize,
}

fn rejected(c: &TimedCommand, e: &CommandError) -> Failure {
    Failure::usage(format!(
        "command #{} at tick {} rejected: {e}",
        c.index, c.tick
    ))
    .with("index", c.index)
    .with("tick", c.tick)
    .with("cmd", &c.cmd)
    .with("reason", e)
}

impl Driver {
    pub fn new(app: App, commands: Vec<TimedCommand>) -> Self {
        Self {
            app,
            pending: commands.into(),
            queued: BTreeMap::new(),
            skipped_seen: 0,
        }
    }

    pub fn world(&mut self) -> &mut World {
        self.app.world_mut()
    }

    /// The next tick to run (ticks completed).
    pub fn tick(&self) -> u64 {
        self.app.world().resource::<SimClock>().tick
    }

    /// Queues the commands due now, then runs one tick; fails when one of
    /// them did not validate as it applied.
    pub fn step(&mut self) -> Result<(), Failure> {
        let now = self.tick();
        self.queued.clear();
        while self.pending.front().is_some_and(|c| c.tick <= now) {
            let c = self.pending.pop_front().expect("front exists");
            let at = Some(c.tick.max(now));
            let key = submit_deferred(self.app.world_mut(), at, c.cmd.clone())
                .map_err(|e| rejected(&c, &e))?;
            self.queued.insert(key, c);
        }
        run_ticks(self.app.world_mut(), 1);
        if let Some(skipped) = self.app.world().get_resource::<SkippedCommands>() {
            let new = skipped.entries.get(self.skipped_seen..).unwrap_or_default();
            self.skipped_seen = skipped.entries.len();
            let failed = new
                .iter()
                .find_map(|s| self.queued.get(&(s.tick, s.seq)).map(|c| (c, &s.error)));
            if let Some((c, error)) = failed {
                return Err(rejected(c, error));
            }
        }
        Ok(())
    }

    /// Steps until `tick` ticks are done.
    pub fn run_to(&mut self, tick: u64) -> Result<(), Failure> {
        while self.tick() < tick {
            self.step()?;
        }
        Ok(())
    }

    /// Commands left unsubmitted (due at or after the current tick).
    pub fn not_reached(&self) -> usize {
        self.pending.len()
    }

    pub fn commands_applied(&self) -> usize {
        self.app
            .world()
            .get_resource::<CommandLog>()
            .map_or(0, |l| l.entries.len())
    }
}

/// The replay recorder of a CLI run of `ticks` ticks: the shared
/// `npc_tools` recorder with checkpoints at multiples of `every` and at the
/// last tick.
pub fn recorder(every: u64, ticks: u64) -> Recorder {
    Recorder::aligned(every, ticks)
}

/// Finishes `recorder`. The header keeps the command line's seed, scenario
/// and `--villagers` as given (the factory's `ScenarioInfo` normalizes the
/// count for fixed-size scenarios), as npcctl always recorded them.
///
/// Other named scenarios keep the factory's unit count (the testbed's
/// scenario files fix their actors); a scenario file becomes a session
/// record (its document, the seed and the replay).
pub fn finish_replay(
    recorder: Recorder,
    scenario: &Scenario,
    world: &mut World,
) -> Result<Recorded, Failure> {
    let mut replay = recorder.finish(world).map_err(Failure::usage)?;
    replay.seed = scenario.seed;
    match &scenario.source {
        Source::Named(_) => {
            replay.scenario.clone_from(&scenario.name);
            if scenario.is_village() {
                replay.villagers = scenario.villagers;
            }
            Ok(Recorded::Replay(replay))
        }
        Source::File { doc, .. } => Ok(Recorded::Session(SessionRecord {
            version: SESSION_VERSION,
            scenario: (**doc).clone(),
            seed: scenario.seed,
            replay,
        })),
    }
}

/// Unit count of a built app (its `ScenarioInfo`).
pub fn units_of(world: &World) -> Option<u32> {
    world.get_resource::<ScenarioInfo>().map(|i| i.villagers)
}

/// Hunger and energy values (milli-units, `0..=100000`).
#[derive(Clone, Copy, Debug, Serialize)]
pub struct NeedPair {
    pub hunger: i32,
    pub energy: i32,
}

/// `{min, max}` of one need over the run.
#[derive(Clone, Copy, Debug, Serialize)]
pub struct Range {
    pub min: i32,
    pub max: i32,
}

/// One day of the `run` timeline, summed over all villagers.
#[derive(Clone, Debug, Serialize)]
pub struct DaySummary {
    /// 0-based day.
    pub day: u64,
    /// Ticks of this day that ran.
    pub ticks: u64,
    /// Times a villager started eating (activity changed to `Eat`).
    pub meals: u32,
    /// Times a villager started working (activity changed to `Work`).
    pub work_shifts: u32,
    /// Villager-ticks spent with activity `Sleep`.
    pub ticks_asleep: u64,
    /// Behaviour-tree aborts (guards interrupting a running branch).
    pub aborts: u32,
    /// Leaves started.
    pub leaf_starts: u32,
    /// Lowest need values seen this day (over all villagers).
    pub min_needs: NeedPair,
    /// Highest need values seen this day.
    pub max_needs: NeedPair,
}

impl DaySummary {
    fn new(day: u64) -> Self {
        Self {
            day,
            ticks: 0,
            meals: 0,
            work_shifts: 0,
            ticks_asleep: 0,
            aborts: 0,
            leaf_starts: 0,
            min_needs: NeedPair {
                hunger: i32::MAX,
                energy: i32::MAX,
            },
            max_needs: NeedPair {
                hunger: i32::MIN,
                energy: i32::MIN,
            },
        }
    }
}

type VillagerQuery = (
    &'static SimId,
    &'static Needs,
    &'static Activity,
    Option<&'static BrainStats>,
);

type BrainQuery = (&'static SimId, &'static BrainStats);

/// Per-day timeline of a run (the logic of the `print_three_day_timeline`
/// test in `village_content/tests/villager.rs`, over every villager).
pub struct Timeline {
    config: SimConfig,
    query: QueryState<VillagerQuery, With<Villager>>,
    /// Per villager: last activity and last `(aborts, leaf_starts)`.
    last: BTreeMap<SimId, (Activity, u32, u32)>,
    /// Other contents: every non-villager brain (aborts and leaf starts).
    others: Option<QueryState<BrainQuery, Without<Villager>>>,
    /// Per brain of `others`: last `(aborts, leaf_starts)`.
    last_other: BTreeMap<SimId, (u32, u32)>,
    /// With `--all-needs`: every entity with `Needs`, every registered need.
    all_needs: Option<AllNeeds>,
    pub days: Vec<DaySummary>,
}

/// Whole-run `{min, max}` of every registered need over every entity with
/// `Needs` (any content), keyed by the need's JSON key (`NeedDef::key`).
struct AllNeeds {
    query: QueryState<&'static Needs>,
    /// Per need id: `(key, min, max)`.
    ranges: Vec<(&'static str, i32, i32)>,
}

impl Timeline {
    pub fn new(world: &mut World) -> Self {
        Self {
            config: world
                .get_resource::<SimConfig>()
                .copied()
                .unwrap_or_default(),
            query: world.query_filtered(),
            last: BTreeMap::new(),
            others: None,
            last_other: BTreeMap::new(),
            all_needs: None,
            days: Vec::new(),
        }
    }

    /// Also tracks every registered need of every entity with `Needs`
    /// ([`Timeline::all_need_ranges`]; `npcctl run --all-needs`).
    pub fn with_all_needs(mut self, world: &mut World) -> Self {
        let ranges = world
            .get_resource::<NeedRegistry>()
            .map(|r| r.defs.iter().map(|d| (d.key, i32::MAX, i32::MIN)).collect())
            .unwrap_or_default();
        self.all_needs = Some(AllNeeds {
            query: world.query(),
            ranges,
        });
        self
    }

    /// Whole-run `{min, max}` per registered need (by `NeedDef::key`, need
    /// ids sorted by name); `None` without [`Timeline::with_all_needs`]. A
    /// need no entity had is `{0, 0}`.
    pub fn all_need_ranges(&self) -> Option<BTreeMap<String, Range>> {
        let all = self.all_needs.as_ref()?;
        Some(
            all.ranges
                .iter()
                .map(|&(key, min, max)| {
                    let seen = min <= max;
                    (
                        key.to_owned(),
                        Range {
                            min: if seen { min } else { 0 },
                            max: if seen { max } else { 0 },
                        },
                    )
                })
                .collect(),
        )
    }

    /// Also counts aborts and leaf starts of non-villager brains (every
    /// content but the village, whose summary stays as it always was).
    pub fn with_other_brains(mut self, world: &mut World) -> Self {
        self.others = Some(world.query_filtered());
        self
    }

    /// Call after tick `tick` ran.
    pub fn observe(&mut self, world: &World, tick: u64) {
        let day_index = self.config.day(tick);
        if self.days.last().is_none_or(|d| d.day != day_index) {
            self.days.push(DaySummary::new(day_index));
        }
        let day = self.days.last_mut().expect("just pushed");
        day.ticks += 1;
        for (&id, needs, &act, stats) in self.query.iter(world) {
            let stats = stats.copied().unwrap_or_default();
            let prev = self.last.get(&id).copied();
            if prev.map(|p| p.0) != Some(act) {
                match act {
                    Activity::Eat => day.meals += 1,
                    Activity::Work => day.work_shifts += 1,
                    _ => {}
                }
            }
            if act == Activity::Sleep {
                day.ticks_asleep += 1;
            }
            let (aborts, starts) = prev.map_or((0, 0), |p| (p.1, p.2));
            day.aborts += stats.aborts.saturating_sub(aborts);
            day.leaf_starts += stats.leaf_starts.saturating_sub(starts);
            let Some(needs) = VillagerNeeds::of(world, needs) else {
                continue;
            };
            let (h, e) = (needs.hunger.value, needs.energy.value);
            day.min_needs.hunger = day.min_needs.hunger.min(h);
            day.min_needs.energy = day.min_needs.energy.min(e);
            day.max_needs.hunger = day.max_needs.hunger.max(h);
            day.max_needs.energy = day.max_needs.energy.max(e);
            self.last.insert(id, (act, stats.aborts, stats.leaf_starts));
        }
        if let Some(all) = &mut self.all_needs {
            for needs in all.query.iter(world) {
                for (range, need) in all.ranges.iter_mut().zip(&needs.values) {
                    range.1 = range.1.min(need.value);
                    range.2 = range.2.max(need.value);
                }
            }
        }
        if let Some(others) = &mut self.others {
            for (&id, stats) in others.iter(world) {
                let (aborts, starts) = self.last_other.get(&id).copied().unwrap_or((0, 0));
                day.aborts += stats.aborts.saturating_sub(aborts);
                day.leaf_starts += stats.leaf_starts.saturating_sub(starts);
                self.last_other
                    .insert(id, (stats.aborts, stats.leaf_starts));
            }
        }
    }

    /// Whole-run `{hunger: {min, max}, energy: {min, max}}`.
    pub fn need_ranges(&self) -> BTreeMap<&'static str, Range> {
        let fold = |get: fn(&NeedPair) -> i32, min: bool| {
            self.days
                .iter()
                .map(|d| get(if min { &d.min_needs } else { &d.max_needs }))
                .filter(|&v| v != i32::MAX && v != i32::MIN)
                .reduce(if min { i32::min } else { i32::max })
                .unwrap_or(0)
        };
        BTreeMap::from([
            (
                "hunger",
                Range {
                    min: fold(|p| p.hunger, true),
                    max: fold(|p| p.hunger, false),
                },
            ),
            (
                "energy",
                Range {
                    min: fold(|p| p.energy, true),
                    max: fold(|p| p.energy, false),
                },
            ),
        ])
    }
}
