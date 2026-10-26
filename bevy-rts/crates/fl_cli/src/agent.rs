//! The agent debugging commands (spec §15 C): what they start from ([`Source`]), the headless
//! [`Session`] they step, and `run-scenario`, `replay-verify` and `replay dump`.
//!
//! Ticks: "at tick T" is the state between ticks when the sim clock reads T, after ticks
//! `0..T` ran. `inspect --tick T`, `save --tick T` and `inspect_world`'s `tick` all mean that.
//! A state hash in a replay is taken *after* its tick ran, before the clock advances.
//!
//! Every command returns a [`Report`]: one JSON document (`--json`) or text, and an exit code.

use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufWriter, Write as _};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use bevy_app::App;
use bevy_ecs::world::World;
use fl_content::ContentDb;
use fl_math::Fx;
use fl_observe::{DebugConfig, JournalSink, RunDir, init_headless, new_run_dir};
use fl_protocol::{DebugWanderCmd, MatchHeader, PlayerCommand, PlayerId, SimId, Tick};
use fl_sim::{
    CommandRegistry, HashInterval, MatchHeaderRes, MatchSetup, Owner, Replay, ReplayRecorder,
    ReplayStart, RunConfig, ScaleScenario, SemanticCheck, SimFeature, SimIndex, SimRunner,
    TestDriver, VerifyOutcome, build_sim_app, compute_state_hash, inspect_entity, inspect_world,
    load_world, save_world, setup_scenario, setup_scenario_def, verify_replay,
};
use fl_view::{CommandDraft, SimEvent, TickDriver, TickOutput};
use serde_json::{Value, json};

use crate::report::{CliError, Report, hash_hex};

/// The game's sim features and the content of the mods.
#[derive(Clone)]
pub struct Game {
    pub features: Vec<Arc<dyn SimFeature>>,
    pub content: Arc<ContentDb>,
}

impl Game {
    /// [`fl_catalog::features`] with the content of `mods`, or of the base mod if `mods` is
    /// empty.
    ///
    /// # Errors
    /// [`CliError::Build`] if the content does not load.
    pub fn load(mods: &[PathBuf]) -> Result<Game, CliError> {
        let features = fl_catalog::features();
        let mods = if mods.is_empty() {
            vec![fl_catalog::base_mod_dir()]
        } else {
            mods.to_vec()
        };
        let content =
            fl_sim::load_content(&features, &mods).map_err(|e| CliError::Build(e.to_string()))?;
        Ok(Game { features, content })
    }
}

/// How a scenario is set up; ignored by other sources.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScenarioOptions {
    /// The match seed instead of the scenario's.
    pub seed: Option<u64>,
    /// Before tick 0, every player orders all its units to wander this many metres
    /// (`debug.wander`; the match then allows cheats).
    pub wander: Option<i32>,
}

impl ScenarioOptions {
    fn is_set(&self) -> bool {
        self.seed.is_some() || self.wander.is_some()
    }
}

/// What a command starts from, by the shape of its argument.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// A scenario of the mods, by id (`base.testbed`).
    ScenarioId(String),
    /// A standalone `*.scenario.ron`.
    ScenarioFile(PathBuf),
    /// A `*.replay`, or the `run.replay` of a run directory.
    Replay(PathBuf),
    /// A `*.save`.
    Save(PathBuf),
}

impl Source {
    /// A directory is a run directory (its `run.replay`); a `.replay` or `.save` file is that;
    /// a `.ron` file is a scenario file; anything else is a scenario id.
    #[must_use]
    pub fn parse(arg: &str) -> Source {
        let path = Path::new(arg);
        if path.is_dir() {
            return Source::Replay(path.join("run.replay"));
        }
        match path.extension().and_then(|ext| ext.to_str()) {
            Some("replay") => Source::Replay(path.to_owned()),
            Some("save") => Source::Save(path.to_owned()),
            Some("ron") => Source::ScenarioFile(path.to_owned()),
            _ => Source::ScenarioId(arg.to_owned()),
        }
    }

    fn is_scenario(&self) -> bool {
        matches!(self, Source::ScenarioId(_) | Source::ScenarioFile(_))
    }
}

/// How a [`Session`] runs the sim.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SessionConfig {
    /// A state hash every this many ticks; `None`: the replay's interval, else the rules'.
    pub hash_interval: Option<u32>,
    /// Keep the journal.
    pub journal: bool,
}

fn read_file(path: &Path) -> Result<Vec<u8>, CliError> {
    std::fs::read(path).map_err(|e| CliError::Io(format!("cannot read {}: {e}", path.display())))
}

/// Read and decode the replay at `path`.
///
/// # Errors
/// [`CliError::Io`] or [`CliError::Setup`] if it does not decode.
pub fn read_replay(path: &Path) -> Result<Replay, CliError> {
    Replay::from_bytes(&read_file(path)?)
        .map_err(|e| CliError::Setup(format!("{}: {e}", path.display())))
}

/// A sim being stepped headless from a [`Source`], recording what it runs as a [`Replay`].
pub struct Session {
    runner: SimRunner,
    driver: Box<dyn TickDriver>,
    recorder: ReplayRecorder,
    /// The tick a replay source ends at: its ticks are not past it.
    end: Option<Tick>,
}

impl Session {
    /// Set up `source` with `game`. A scenario's match is set up with `options` (it is an
    /// error to give them for another source); a scenario file starts its recording from a
    /// save of the world as set up. A replay plays back its recorded inputs up to its end; a
    /// save resumes with its pending commands.
    ///
    /// # Errors
    /// [`CliError::Usage`] for scenario options on another source; otherwise any error
    /// reading, decoding, building or setting up the source.
    pub fn open(
        game: &Game,
        source: &Source,
        options: &ScenarioOptions,
        config: SessionConfig,
    ) -> Result<Session, CliError> {
        if options.is_set() && !source.is_scenario() {
            return Err(CliError::Usage(
                "--seed and --wander only apply to a scenario".to_owned(),
            ));
        }
        let run = RunConfig {
            hash_interval: config.hash_interval,
            journal: Some(config.journal),
            ..RunConfig::default()
        };
        let setup_err = |e: &dyn std::fmt::Display| CliError::Setup(e.to_string());
        let build = |setup: MatchSetup, run: RunConfig| {
            build_sim_app(&game.features, Arc::clone(&game.content), setup, run)
                .map_err(|e| CliError::Build(e.to_string()))
        };
        let scenario_setup = |seed: u64| MatchSetup {
            seed: options.seed.unwrap_or(seed),
            cheats_allowed: options.wander.is_some(),
        };
        let (app, start, driver, end): (App, ReplayStart, Box<dyn TickDriver>, _) = match source {
            Source::ScenarioId(id) => {
                let setup =
                    MatchSetup::for_scenario(&game.content, id).map_err(|e| setup_err(&e))?;
                let mut app = build(scenario_setup(setup.seed), run)?;
                setup_scenario(app.world_mut(), id).map_err(|e| setup_err(&e))?;
                let start = ReplayStart::Scenario(id.clone());
                (app, start, Box::new(TestDriver::new()), None)
            }
            Source::ScenarioFile(path) => {
                let def = game
                    .content
                    .load_scenario_file(path)
                    .map_err(|e| CliError::Setup(format!("{}:\n{e}", path.display())))?;
                let mut app = build(scenario_setup(def.seed), run)?;
                setup_scenario_def(app.world_mut(), &def).map_err(|e| setup_err(&e))?;
                // The scenario is not in the content: the replay starts from the world as set up.
                let bytes = save_world(app.world(), &[]).map_err(|e| setup_err(&e))?;
                let start = ReplayStart::Save(bytes);
                (app, start, Box::new(TestDriver::new()), None)
            }
            Source::Replay(path) => {
                let replay = read_replay(path)?;
                if !replay.reproducible() {
                    return Err(CliError::Setup(format!(
                        "{} is not reproducible: the content was reloaded during the match",
                        path.display()
                    )));
                }
                let run = RunConfig {
                    hash_interval: config.hash_interval.or(Some(replay.hash_interval)),
                    ..run
                };
                let app = replay_app(game, &replay, run)?;
                let inputs = fl_sim::replay_inputs(&replay, &app)
                    .map_err(|e| CliError::Setup(e.to_string()))?;
                let first = app.world().resource::<fl_sim::SimClock>().tick;
                let end = Tick(first.0 + replay.ticks);
                let driver = fl_sim::ReplayDriver::new(inputs, first, Some(end));
                (app, replay.start, Box::new(driver), Some(end))
            }
            Source::Save(path) => {
                let bytes = read_file(path)?;
                let loaded = load_world(&game.features, Arc::clone(&game.content), &bytes, run)
                    .map_err(|e| CliError::Setup(format!("{}: {e}", path.display())))?;
                let first = loaded.app.world().resource::<fl_sim::SimClock>().tick;
                let mut driver = TestDriver::new().starting_at(first);
                driver.restore(loaded.pending, &loaded.last_seq);
                (loaded.app, ReplayStart::Save(bytes), Box::new(driver), None)
            }
        };
        let header: MatchHeader = app.world().resource::<MatchHeaderRes>().0.clone();
        let interval = app.world().resource::<HashInterval>().0;
        let mut session = Session {
            runner: SimRunner::new(app),
            driver,
            recorder: ReplayRecorder::new(header, start, interval),
            end,
        };
        if let Some(radius) = options.wander {
            session.issue_wander(radius);
        }
        Ok(session)
    }

    /// Every player orders all its units to wander `radius` metres, from the next tick on.
    fn issue_wander(&mut self, radius: i32) {
        let world = self.runner.world();
        let mut owned: BTreeMap<PlayerId, Vec<SimId>> = BTreeMap::new();
        for (id, entity) in world.resource::<SimIndex>().iter() {
            if let Some(Owner(player)) = world.get::<Owner>(entity) {
                owned.entry(*player).or_default().push(id);
            }
        }
        let kind = world.resource::<CommandRegistry>().encode(&DebugWanderCmd {
            radius: Fx::from_int(radius),
        });
        for (player, units) in owned {
            self.driver.issue(CommandDraft {
                player,
                units,
                kind: kind.clone(),
                queue: false,
            });
        }
    }

    /// The tick the next [`Session::step`] runs: the sim clock.
    #[must_use]
    pub fn next_tick(&self) -> Tick {
        self.runner.next_tick()
    }

    /// Where a replay source ends.
    #[must_use]
    pub fn end(&self) -> Option<Tick> {
        self.end
    }

    /// Run one tick and record it.
    ///
    /// # Errors
    /// [`CliError::Usage`] past the end of a replay source.
    pub fn step(&mut self) -> Result<TickOutput, CliError> {
        let input = self.driver.poll().ok_or_else(|| {
            CliError::Usage(format!(
                "the replay ends at tick {}",
                self.end.map_or(0, |end| end.0)
            ))
        })?;
        let output = self.runner.step(input.clone());
        self.recorder.record(&input, &output);
        Ok(output)
    }

    /// Run until the clock reads `tick`.
    ///
    /// # Errors
    /// [`CliError::Usage`] if `tick` is before the clock or past a replay source's end.
    pub fn run_to(&mut self, tick: u32) -> Result<(), CliError> {
        let now = self.next_tick().0;
        if tick < now {
            return Err(CliError::Usage(format!(
                "tick {tick} is before the start, tick {now}"
            )));
        }
        if let Some(end) = self.end
            && tick > end.0
        {
            return Err(CliError::Usage(format!(
                "tick {tick} is past the replay's end, tick {}",
                end.0
            )));
        }
        while self.next_tick().0 < tick {
            self.step()?;
        }
        Ok(())
    }

    /// The sim world, between ticks.
    #[must_use]
    pub fn world(&self) -> &World {
        self.runner.world()
    }

    /// The commands the driver holds for later ticks (a replay's: its remaining inputs).
    #[must_use]
    pub fn pending(&self) -> Vec<PlayerCommand> {
        self.driver.pending()
    }

    /// The ticks run so far, as a replay.
    #[must_use]
    pub fn replay(&self) -> Replay {
        self.recorder.clone().finish()
    }
}

/// The sim of `replay`, set up per its start by [`fl_sim::replay_app`] (the steps
/// [`verify_replay`] takes); its header warnings are logged.
fn replay_app(game: &Game, replay: &Replay, run: RunConfig) -> Result<App, CliError> {
    let (app, warnings) =
        fl_sim::replay_app(replay, &game.features, Arc::clone(&game.content), run)
            .map_err(|e| CliError::Setup(e.to_string()))?;
    for warning in warnings {
        tracing::warn!("{warning}");
    }
    Ok(app)
}

/// Install the process's logging: `log_filter` to stderr and, with `run`, to its
/// `log.jsonl`. Only the first call has an effect.
pub fn init_logs(log_filter: &str, run: Option<&RunDir>) {
    init_headless(
        &DebugConfig {
            log_filter: log_filter.to_owned(),
            json_log: run.is_some(),
            journal_file: false,
            profiler: false,
            ..DebugConfig::default()
        },
        run,
    );
}

/// What `run-scenario` runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunScenario {
    /// A scenario id or a `*.scenario.ron` path.
    pub scenario: String,
    pub options: ScenarioOptions,
    pub ticks: u32,
    /// Log at `trace` level instead of `info,fl_sim=debug`.
    pub trace: bool,
    /// Write the world and every entity to `states.jsonl` whenever the clock is a multiple of
    /// this (from tick 0).
    pub dump_state_every: Option<u32>,
    /// The run directory is created in here.
    pub out: PathBuf,
}

/// One `states.jsonl` line: `{"world": inspect_world, "entities": [inspect_entity, ...]}`.
fn state_line(world: &World) -> Value {
    let entities: Vec<Value> = world
        .resource::<SimIndex>()
        .ids()
        .filter_map(|id| inspect_entity(world, id))
        .collect();
    json!({ "world": inspect_world(world), "entities": entities })
}

fn io_err(path: &Path) -> impl Fn(std::io::Error) -> CliError + '_ {
    move |e| CliError::Io(format!("{}: {e}", path.display()))
}

/// `run-scenario`: run a scenario headless for `ticks` ticks into a new run directory
/// `<out>/<run id>/` holding `log.jsonl` (the logs), `journal.jsonl`, `run.replay` and
/// `states.jsonl` (empty without `dump_state_every`). Logs also go to stderr.
///
/// JSON: `{run_dir, ticks, final_hash, rules_hash, journal_entries, rejected_commands}`.
///
/// # Errors
/// [`CliError::Usage`] if the source is not a scenario or `dump_state_every` is 0; any error
/// setting it up or writing the files.
pub fn run_scenario(game: &Game, args: &RunScenario) -> Result<Report, CliError> {
    let source = Source::parse(&args.scenario);
    if !source.is_scenario() {
        return Err(CliError::Usage(format!(
            "run-scenario runs a scenario id or a *.scenario.ron, not {}",
            args.scenario
        )));
    }
    if args.dump_state_every == Some(0) {
        return Err(CliError::Usage(
            "--dump-state-every must be at least 1".to_owned(),
        ));
    }
    // Check the scenario exists before a run directory is created for it.
    match &source {
        Source::ScenarioId(id) => {
            MatchSetup::for_scenario(&game.content, id)
                .map_err(|e| CliError::Setup(e.to_string()))?;
        }
        Source::ScenarioFile(path) => {
            game.content
                .load_scenario_file(path)
                .map_err(|e| CliError::Setup(format!("{}:\n{e}", path.display())))?;
        }
        Source::Replay(_) | Source::Save(_) => {}
    }
    let run_dir = new_run_dir(&args.out).map_err(io_err(&args.out))?;
    let log_filter = if args.trace {
        "trace"
    } else {
        "info,fl_sim=debug"
    };
    init_logs(log_filter, Some(&run_dir));
    let mut session = Session::open(
        game,
        &source,
        &args.options,
        SessionConfig {
            hash_interval: None,
            journal: true,
        },
    )?;

    let dir = &run_dir.path;
    let mut journal = JournalSink::create(&run_dir).map_err(io_err(dir))?;
    let states_path = dir.join("states.jsonl");
    let mut states = BufWriter::new(File::create(&states_path).map_err(io_err(&states_path))?);
    let mut dump = |world: &World| -> Result<(), CliError> {
        if let Some(every) = args.dump_state_every
            && world
                .resource::<fl_sim::SimClock>()
                .tick
                .0
                .is_multiple_of(every)
        {
            writeln!(states, "{}", state_line(world)).map_err(io_err(&states_path))?;
        }
        Ok(())
    };
    dump(session.world())?;
    let (mut journal_entries, mut rejected) = (0_u64, 0_u64);
    for _ in 0..args.ticks {
        let output = session.step()?;
        journal.write(&output.journal).map_err(io_err(dir))?;
        journal_entries += output.journal.len() as u64;
        rejected += output
            .events
            .iter()
            .filter(|event| matches!(event, SimEvent::CommandRejected { .. }))
            .count() as u64;
        dump(session.world())?;
    }
    drop(dump);
    states.flush().map_err(io_err(&states_path))?;
    let replay_path = dir.join("run.replay");
    std::fs::write(&replay_path, session.replay().to_bytes()).map_err(io_err(&replay_path))?;

    let final_hash = hash_hex(compute_state_hash(session.world()));
    let rules_hash = hash_hex(game.content.rules_hash());
    let json = json!({
        "run_dir": dir,
        "ticks": args.ticks,
        "final_hash": final_hash,
        "rules_hash": rules_hash,
        "journal_entries": journal_entries,
        "rejected_commands": rejected,
    });
    let text = format!(
        "run dir: {}\nticks: {}\nfinal hash: {final_hash}\nrules hash: {rules_hash}\n\
         journal entries: {journal_entries}\nrejected commands: {rejected}\n",
        dir.display(),
        args.ticks
    );
    Ok(Report::ok(json, text))
}

/// `replay-verify`: play the replay at `path` back, compare its hashes and run its semantic
/// checks. Exit 0 with `{"status": "verified", "ticks", "warnings"}`; exit 1 with
/// `{"status": "diverged", "tick", "expected", "actual"}` at the earliest differing hash, or
/// with `{"status": "semantic-failed", "failures": [{"id", "tick", "detail"}]}`; exit 3 with
/// `{"status": "non-reproducible"}` for a replay whose content was reloaded.
///
/// # Errors
/// If the replay cannot be read or played back.
pub fn replay_verify(game: &Game, path: &Path) -> Result<Report, CliError> {
    let replay = read_replay(path)?;
    let outcome = verify_replay(&replay, &game.features, Arc::clone(&game.content))
        .map_err(|e| CliError::Setup(format!("{}: {e}", path.display())))?;
    Ok(match outcome {
        VerifyOutcome::Verified { ticks, warnings } => {
            let mut text = format!("verified: {ticks} ticks\n");
            for warning in &warnings {
                text.push_str(&format!("warning: {warning}\n"));
            }
            Report::ok(
                json!({"status": "verified", "ticks": ticks, "warnings": warnings}),
                text,
            )
        }
        VerifyOutcome::Diverged(d) => {
            let (expected, actual) = (hash_hex(d.expected), hash_hex(d.actual));
            Report {
                json: json!({
                    "status": "diverged", "tick": d.tick.0,
                    "expected": expected, "actual": actual,
                }),
                text: format!(
                    "diverged at tick {}: expected {expected}, actual {actual}\n",
                    d.tick.0
                ),
                code: 1,
            }
        }
        VerifyOutcome::SemanticFailed(failures) => {
            let mut text = String::new();
            for f in &failures {
                text.push_str(&format!(
                    "semantic check {} failed at tick {}: {}\n",
                    f.id, f.tick.0, f.detail
                ));
            }
            let failures: Vec<Value> = failures
                .iter()
                .map(|f| json!({"id": f.id, "tick": f.tick.0, "detail": f.detail}))
                .collect();
            Report {
                json: json!({"status": "semantic-failed", "failures": failures}),
                text,
                code: 1,
            }
        }
        VerifyOutcome::NonReproducible => Report {
            json: json!({"status": "non-reproducible"}),
            text: "non-reproducible: the content was reloaded during the match\n".to_owned(),
            code: 3,
        },
        other => {
            return Err(CliError::Check(format!(
                "{}: unknown verify outcome {other:?}",
                path.display()
            )));
        }
    })
}

/// `replay dump`: the replay at `path` as [`Replay::to_json`] (a format-3 replay as read into
/// the current format); the text is a summary.
///
/// # Errors
/// If the replay cannot be read.
pub fn replay_dump(path: &Path) -> Result<Report, CliError> {
    let replay = read_replay(path)?;
    let start = match &replay.start {
        ReplayStart::Scenario(id) => format!("scenario {id}"),
        ReplayStart::Scale { version, params } => match ScaleScenario::decode(*version, params) {
            Ok(scale) => format!("scale scenario of {} units", scale.units),
            Err(err) => format!("scale scenario ({err})"),
        },
        ReplayStart::Save(bytes) => format!("save of {} bytes", bytes.len()),
    };
    let commands: usize = replay.inputs.iter().map(|i| i.commands.len()).sum();
    let checks: Vec<&str> = replay.semantic.iter().map(SemanticCheck::id).collect();
    let text = format!(
        "format version: {}\nseed: {}\ncheats allowed: {}\nrules hash: {}\nstart: {start}\n\
         ticks: {}\nhash interval: {}\ninputs: {} ({commands} commands)\nhashes: {}\n\
         events: {}\nsemantic checks: {}\njournal records: {}\n",
        replay.version,
        replay.header.seed,
        replay.header.cheats_allowed,
        hash_hex(replay.header.rules_hash),
        replay.ticks,
        replay.hash_interval,
        replay.inputs.len(),
        replay.hashes.len(),
        replay.events.len(),
        if checks.is_empty() {
            "none".to_owned()
        } else {
            checks.join(", ")
        },
        replay
            .journal_summary
            .values()
            .map(|&n| u64::from(n))
            .sum::<u64>(),
    );
    Ok(Report::ok(replay.to_json(), text))
}
