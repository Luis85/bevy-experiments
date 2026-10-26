//! `npcctl`: the headless JSON CLI of the NPC framework (spec §3.5).
//!
//! JSON goes to stdout (or `--out FILE`, `--pretty` to indent), logs to
//! stderr. Exit codes: 0 ok, 1 check failed (replay mismatch, divergence,
//! bench regression), 2 usage or input error. Every failure also prints a
//! JSON object with an `"error"` string on stdout (clap usage errors print
//! only clap's message on stderr).

mod agent;
mod explorer;
mod sim;

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{ArgAction, ArgGroup, Args, Parser, Subcommand, ValueEnum};
use npc_framework::kernel::inspect as kernel_inspect;
use npc_framework::kernel::journal::{JournalFilter, JournalKind, JournalSink, set_journal};
use npc_framework::perf::{MechanicTags, PerfCollector};
use npc_framework::prelude::{CommandError, Journal, SimId};
use npc_framework::tools::bench;
use npc_framework::tools::commands_file::load_optional_commands;
use serde::Serialize;
use serde_json::Value;
use tracing_subscriber::Layer;
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::layer::SubscriberExt;
use village_content::inspect;
use village_content::replay::Replay;

use npc_framework::tools::output::{
    EXIT_CHECK_FAILED, EXIT_OK, Failure, Out, read_json, write_json_file,
};

use crate::sim::{
    DaySummary, Driver, Range, Recorded, Scenario, Timeline, content_of, finish_replay,
    is_session_record, recorder, units_of, verify_replay,
};

/// `run` output.
#[derive(Serialize)]
struct RunSummary<'a> {
    seed: u64,
    scenario: String,
    villagers: u32,
    ticks: u64,
    day_length: u64,
    final_tick: u64,
    final_hash: u64,
    days: &'a [DaySummary],
    needs: std::collections::BTreeMap<&'static str, Range>,
    /// With `--all-needs`: every registered need (by its JSON key) over
    /// every entity with needs, any content. Absent otherwise, so the
    /// default summary is unchanged.
    #[serde(skip_serializing_if = "Option::is_none")]
    needs_all: Option<std::collections::BTreeMap<String, Range>>,
    aborts: u32,
    leaf_starts: u32,
    commands_applied: usize,
    commands_not_reached: usize,
    journal: Option<String>,
    record: Option<String>,
    /// Content-specific end-of-run state (absent for the village).
    #[serde(skip_serializing_if = "Option::is_none")]
    content_summary: Option<Value>,
}

/// `diverge` output.
#[derive(Serialize)]
struct DivergeOutput<'a> {
    first_divergent_tick: Option<u64>,
    ticks_compared: u64,
    entities_differing: usize,
    truncated: bool,
    entities: Vec<DivergeEntity<'a>>,
}

#[derive(Serialize)]
struct DivergeEntity<'a> {
    sim_id: SimId,
    components_differing: &'a [String],
    a: &'a Value,
    b: &'a Value,
}

#[derive(Parser, Debug)]
#[command(
    name = "npcctl",
    version,
    about = "Headless JSON CLI for the NPC framework: run scenarios, inspect units, \
             filter the journal, verify replays, find divergences, benchmark.",
    after_help = "JSON goes to stdout (or --out FILE), logs to stderr.\n\
                  Exit codes: 0 ok, 1 check failed, 2 usage or input error.\n\
                  Example: npcctl run --seed 1 --ticks 7200 --pretty"
)]
struct Cli {
    /// Indent the JSON output (JSON Lines output is never indented).
    #[arg(long, global = true)]
    pretty: bool,
    /// Write the JSON output to FILE instead of stdout.
    #[arg(long, global = true, value_name = "FILE")]
    out: Option<PathBuf>,
    /// Log level on stderr.
    #[arg(long, global = true, value_enum, default_value_t = LogLevel::Warn)]
    log: LogLevel,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Copy, Clone, Debug, ValueEnum)]
enum LogLevel {
    Off,
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

impl LogLevel {
    fn filter(self) -> LevelFilter {
        match self {
            LogLevel::Off => LevelFilter::OFF,
            LogLevel::Error => LevelFilter::ERROR,
            LogLevel::Warn => LevelFilter::WARN,
            LogLevel::Info => LevelFilter::INFO,
            LogLevel::Debug => LevelFilter::DEBUG,
            LogLevel::Trace => LevelFilter::TRACE,
        }
    }
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// Run a scenario; print a per-day timeline, need ranges, aborts and the final hash.
    #[command(
        after_help = "Example: npcctl run --seed 1 --ticks 7200 --record replay.json --pretty"
    )]
    Run(RunArgs),
    /// Snapshot one unit or the whole world after T ticks.
    #[command(after_help = "Example: npcctl inspect --seed 1 --tick 1500 --unit 5 --pretty")]
    Inspect(InspectArgs),
    /// Run a scenario with the journal on and print matching entries as JSON Lines.
    #[command(
        after_help = "Example: npcctl journal --seed 1 --ticks 3000 --unit 5 --kind LeafStart,Abort --limit 20"
    )]
    Journal(JournalArgs),
    /// Record or verify replays (seed + scenario + commands + hash checkpoints).
    #[command(subcommand, after_help = "Example: npcctl replay verify replay.json")]
    Replay(ReplayCmd),
    /// Find the first tick where two runs (replay files or seeds) differ.
    #[command(after_help = "Example: npcctl diverge --seed-a 1 --seed-b 2 --ticks 3000 --pretty")]
    #[command(group(ArgGroup::new("mode").required(true).args(["a", "seed_a"])))]
    Diverge(DivergeArgs),
    /// Measure ticks/s and per-mechanic p50/p95/max at several village sizes.
    #[command(
        after_help = "Example: npcctl bench --villagers 1,100,500 --ticks 2000 --out bench/baseline.json\n\
                      (build with --release for meaningful numbers)"
    )]
    Bench(BenchArgs),
    /// Serve a scenario headless, in real time, for external agents (JSON-RPC over HTTP).
    #[command(
        after_help = "Example: npcctl serve --scenario content/village_content/scenarios/agent_village.json --tick-hz 20 --record session.json
                      Prints {\"listening\": url, ..} once ready; `npcctl session stop` ends it."
    )]
    Serve(agent::ServeArgs),
    /// Control units of a running session or game (one JSON-RPC call per invocation).
    Agent {
        #[command(flatten)]
        client: agent::ClientArgs,
        #[command(subcommand)]
        cmd: agent::AgentCmd,
    },
    /// The player's notes of a unit (diary, memory, proposals; no controller): list, add, update
    /// (only the player sets a proposal's --status).
    #[command(
        after_help = "Examples:\n  npcctl notes list --unit 33 --book proposals\n  npcctl notes add --unit 33 --book memory --title \"The store\" --body \"Locked.\"\n  npcctl notes update --unit 33 --book proposals --id 2 --expected-tick 480 --expected-date 2026-10-25T10:00:00Z --status liked"
    )]
    Notes {
        #[command(flatten)]
        client: agent::ClientArgs,
        #[command(subcommand)]
        cmd: agent::NotesCmd,
    },
    /// Read or reset the world of a running session.
    World {
        #[command(flatten)]
        client: agent::ClientArgs,
        #[command(subcommand)]
        cmd: agent::WorldCmd,
    },
    /// Save a scenario's world to a snapshot file, or check a file against this build.
    #[command(subcommand)]
    Snapshot(SnapshotCmd),
    /// Inspect or stop a running session.
    Session {
        #[command(flatten)]
        client: agent::ClientArgs,
        #[command(subcommand)]
        cmd: agent::SessionCmd,
    },
    /// A stdio MCP server (tools observe, act, cancel, events, attach, list_units, notes_list,
    /// notes_add, notes_update) for a session.
    #[command(
        after_help = "Example (MCP client config): {\"command\": \"npcctl\", \"args\": [\"mcp\", \"--url\", \"http://127.0.0.1:15702\", \"--controller\", \"explorer\"]}"
    )]
    Mcp {
        #[arg(long, default_value = agent::DEFAULT_URL)]
        url: String,
        /// Bearer token of a session started with `serve --token`.
        #[arg(long, value_name = "TOKEN")]
        token: Option<String>,
        /// The controller the notes tools use when a call names none.
        #[arg(long, default_value = npc_framework::agent_io::mcp::DEFAULT_CONTROLLER)]
        controller: String,
    },
}

#[derive(Subcommand, Debug)]
enum ReplayCmd {
    /// Re-run a replay file and compare every checkpoint: {ok, checked, first_mismatch}.
    #[command(after_help = "Example: npcctl replay verify replay.json")]
    Verify {
        /// Replay JSON file.
        file: PathBuf,
    },
    /// Run a scenario and print its replay JSON (use --out FILE to save it).
    #[command(
        after_help = "Example: npcctl replay record --seed 1 --ticks 3000 --checkpoint-every 500 --out replay.json"
    )]
    Record(RecordArgs),
    /// Re-run an agent session record (`serve --record`) without the agent: {ok, checked, first_mismatch}.
    VerifySession {
        /// Session record JSON file.
        file: PathBuf,
    },
}

#[derive(Subcommand, Debug)]
enum SnapshotCmd {
    /// Run a scenario for --ticks and save its world to --out FILE (atomically):
    /// {ok, file, scenario, seed, tick, entities}. Here --out is the snapshot file,
    /// not the JSON output (which goes to stdout).
    #[command(
        after_help = "Example: npcctl snapshot save --scenario testbed --seed 1 --ticks 600 --out target/save.json"
    )]
    Save {
        #[command(flatten)]
        world: WorldArgs,
        /// Ticks to run before saving.
        #[arg(long, default_value_t = 0)]
        ticks: u64,
    },
    /// Check a snapshot file against the build of the scenario (format version, then
    /// content hash) without loading it: {ok, error?}; exit 1 when it does not fit,
    /// 2 when the file is unreadable.
    #[command(after_help = "Example: npcctl snapshot check --scenario testbed target/save.json")]
    Check {
        #[command(flatten)]
        world: WorldArgs,
        /// Snapshot file.
        file: PathBuf,
    },
}

/// World selection shared by the scenario-running subcommands.
#[derive(Args, Debug)]
struct WorldArgs {
    /// World seed [default: 1; a scenario file: its own seed, else 1].
    #[arg(long)]
    seed: Option<u64>,
    /// Villager count (each gets its own plot in `village_n`).
    #[arg(long, default_value_t = 1)]
    villagers: u32,
    /// `village`, `village_n`, `village_nav`, `testbed`, `testbed:<name>`
    /// (an embedded testbed scenario: doors, smithy, vehicles, ..) or a
    /// scenario file (`*.json`) [default: village for 1 villager, else village_n].
    #[arg(long)]
    scenario: Option<String>,
    /// JSON array of commands: `{"tick":T,"cmd":{..}}` items or replay
    /// `[tick, seq, {..}]` tuples (or a replay file, whose commands are used).
    #[arg(long, value_name = "FILE")]
    commands: Option<PathBuf>,
}

impl WorldArgs {
    fn scenario(&self) -> Result<Scenario, Failure> {
        Scenario::new(self.seed, self.scenario.as_deref(), self.villagers)
    }
}

#[derive(Args, Debug)]
struct RunArgs {
    #[command(flatten)]
    world: WorldArgs,
    /// Ticks to run (2400 per day).
    #[arg(long)]
    ticks: u64,
    /// Stream the whole journal to FILE as JSON Lines.
    #[arg(long, value_name = "FILE")]
    journal: Option<PathBuf>,
    /// Also journal every condition evaluation (CondEval).
    #[arg(long)]
    verbose: bool,
    /// Save the run as a replay to FILE.
    #[arg(long, value_name = "FILE")]
    record: Option<PathBuf>,
    /// Replay checkpoint interval in ticks (0: only the final tick).
    #[arg(long, default_value_t = 100, value_name = "N")]
    checkpoint_every: u64,
    /// Also report `needs_all`: the whole-run {min, max} of every
    /// registered need (keyed by its JSON key) over every entity with
    /// needs, for any content (`needs` stays the village's hunger/energy).
    #[arg(long)]
    all_needs: bool,
}

#[derive(Args, Debug)]
#[command(group(ArgGroup::new("what").args(["unit", "all"])))]
struct InspectArgs {
    #[command(flatten)]
    world: WorldArgs,
    /// Ticks to run before the snapshot.
    #[arg(long)]
    tick: u64,
    /// SimId of the unit (a UnitSnapshot with its recent journal).
    #[arg(long, value_name = "ID")]
    unit: Option<u32>,
    /// The whole world (a WorldSnapshot); the default without --unit.
    #[arg(long)]
    all: bool,
    /// Journal from before the scenario spawns, so `recent_journal` may hold
    /// its tick-0 Spawn entries.
    #[arg(long, value_name = "BOOL", default_value_t = false, action = ArgAction::Set)]
    journal_from_build: bool,
}

#[derive(Args, Debug)]
struct JournalArgs {
    #[command(flatten)]
    world: WorldArgs,
    /// Ticks to run (stops early after --to).
    #[arg(long)]
    ticks: u64,
    /// Only entries about this SimId.
    #[arg(long, value_name = "ID")]
    unit: Option<u32>,
    /// Only these kinds (comma-separated, case-insensitive), e.g. LeafStart,Abort.
    #[arg(long, value_delimiter = ',', value_name = "K,..")]
    kind: Vec<String>,
    /// First tick, inclusive.
    #[arg(long, value_name = "T1")]
    from: Option<u64>,
    /// Last tick, inclusive.
    #[arg(long, value_name = "T2")]
    to: Option<u64>,
    /// Case-sensitive substring of the entry's JSON line.
    #[arg(long, value_name = "TEXT")]
    grep: Option<String>,
    /// Keep only the last N matches.
    #[arg(long, value_name = "N")]
    limit: Option<usize>,
    /// Also record condition evaluations (CondEval entries).
    #[arg(long)]
    verbose: bool,
    /// Journal from before the scenario spawns (adds its tick-0 Spawn
    /// entries); `false` starts the journal after the app is built.
    #[arg(long, value_name = "BOOL", default_value_t = true, action = ArgAction::Set)]
    journal_from_build: bool,
}

#[derive(Args, Debug)]
struct RecordArgs {
    #[command(flatten)]
    world: WorldArgs,
    /// Ticks to run.
    #[arg(long)]
    ticks: u64,
    /// Checkpoint interval in ticks (0: only the final tick).
    #[arg(long, default_value_t = 100, value_name = "N")]
    checkpoint_every: u64,
}

#[derive(Args, Debug)]
struct DivergeArgs {
    /// Replay file of run A.
    #[arg(long, value_name = "FILE", requires = "b", conflicts_with_all = ["seed_a", "seed_b"])]
    a: Option<PathBuf>,
    /// Replay file of run B.
    #[arg(long, value_name = "FILE", requires = "a")]
    b: Option<PathBuf>,
    /// Seed of run A (same scenario and commands as B).
    #[arg(long, value_name = "N", requires = "seed_b")]
    seed_a: Option<u64>,
    /// Seed of run B.
    #[arg(long, value_name = "M", requires = "seed_a")]
    seed_b: Option<u64>,
    /// Ticks to compare [default: 2400 with seeds; with files, the shorter replay].
    #[arg(long)]
    ticks: Option<u64>,
    /// Villager count (seed mode).
    #[arg(long, default_value_t = 1, conflicts_with = "a")]
    villagers: u32,
    /// Scenario (seed mode), as for `run --scenario` (not a file)
    /// [default: village for 1 villager, else village_n].
    #[arg(long, conflicts_with = "a")]
    scenario: Option<String>,
    /// Commands applied to both runs (seed mode), as for `run --commands`.
    #[arg(long, value_name = "FILE", conflicts_with = "a")]
    commands: Option<PathBuf>,
    /// Report at most N differing entities.
    #[arg(long, default_value_t = 20, value_name = "N")]
    max_entities: usize,
}

#[derive(Args, Debug)]
struct BenchArgs {
    /// Village sizes, comma-separated.
    #[arg(
        long,
        value_delimiter = ',',
        default_value = "1,100,500",
        value_name = "N,.."
    )]
    villagers: Vec<u32>,
    /// Measured ticks per size (also the stats window, so p95 covers them all).
    #[arg(long, default_value_t = 2000)]
    ticks: u64,
    /// Unmeasured ticks run first at each size.
    #[arg(long, default_value_t = 200)]
    warmup: u64,
    #[arg(long, default_value_t = 1)]
    seed: u64,
    /// Scenario to measure: `village_n` (sized by --villagers) or another
    /// named scenario such as `testbed` (fixed size: measured once).
    #[arg(long, default_value = "village_n")]
    scenario: String,
    /// Systems listed per size.
    #[arg(long, default_value_t = 15)]
    top: usize,
    /// Compare with this bench JSON; exit 1 when a mechanic regressed (see --metric).
    #[arg(long, value_name = "FILE")]
    baseline: Option<PathBuf>,
    /// Statistic compared against the baseline.
    #[arg(long, value_enum, default_value_t = bench::Metric::P95)]
    metric: bench::Metric,
    /// Regression threshold on the compared statistic, in percent.
    #[arg(long, default_value_t = 25.0, value_name = "PCT")]
    threshold_pct: f64,
    /// Ignore p95 increases smaller than this many µs (scheduler and timer noise).
    #[arg(long, default_value_t = 5.0, value_name = "US")]
    min_us: f64,
    /// Skip mechanics with fewer samples than this (sparse ones like
    /// think.start at 1 villager, whose p95 is just their max).
    #[arg(long, default_value_t = 100, value_name = "N")]
    min_samples: u64,
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    // The perf layer must be the global subscriber before any app (and any
    // system span) exists: spans created without a subscriber stay disabled.
    let collector = match &cli.cmd {
        Cmd::Bench(b) => {
            let collector = PerfCollector::new(b.ticks.max(1) as usize);
            let mut tags = MechanicTags::default_npc();
            village_content::perf_tags(&mut tags);
            testbed_content::perf_tags(&mut tags);
            collector.set_tags(tags);
            Some(collector)
        }
        _ => None,
    };
    let stderr_log = tracing_subscriber::fmt::layer()
        .with_writer(std::io::stderr)
        .with_filter(cli.log.filter());
    let subscriber = tracing_subscriber::registry()
        .with(collector.as_ref().map(PerfCollector::layer))
        .with(stderr_log);
    if tracing::subscriber::set_global_default(subscriber).is_err() {
        eprintln!("npcctl: a global tracing subscriber was already set");
    }

    // `snapshot save` uses --out as the snapshot file: its JSON goes to stdout.
    let json_path = match &cli.cmd {
        Cmd::Snapshot(SnapshotCmd::Save { .. }) => None,
        _ => cli.out.clone(),
    };
    let mut out = Out::new(json_path, cli.pretty);
    let result = match &cli.cmd {
        Cmd::Run(a) => cmd_run(a, &mut out, cli.pretty),
        Cmd::Inspect(a) => cmd_inspect(a, &mut out),
        Cmd::Journal(a) => cmd_journal(a, &mut out),
        Cmd::Replay(ReplayCmd::Verify { file }) => cmd_verify(file, &mut out),
        Cmd::Replay(ReplayCmd::Record(a)) => cmd_record(a, &mut out),
        Cmd::Diverge(a) => cmd_diverge(a, &mut out),
        Cmd::Bench(a) => cmd_bench(a, collector.as_ref().expect("bench collector"), &mut out),
        Cmd::Replay(ReplayCmd::VerifySession { file }) => agent::cmd_verify_session(file, &mut out),
        Cmd::Serve(a) => agent::cmd_serve(a, &mut out),
        Cmd::Agent { client, cmd } => agent::cmd_agent(cmd, client, &mut out),
        Cmd::World { client, cmd } => agent::cmd_world(cmd, client, &mut out),
        Cmd::Session { client, cmd } => agent::cmd_session(cmd, client, &mut out),
        Cmd::Notes { client, cmd } => agent::cmd_notes(cmd, client, &mut out),
        Cmd::Snapshot(SnapshotCmd::Save { world, ticks }) => {
            cmd_snapshot_save(world, *ticks, cli.out.as_deref(), &mut out)
        }
        Cmd::Snapshot(SnapshotCmd::Check { world, file }) => {
            cmd_snapshot_check(world, file, &mut out)
        }
        Cmd::Mcp {
            url,
            token,
            controller,
        } => agent::cmd_mcp(url, token.as_deref(), controller),
    }
    .and_then(|code| out.finish().map(|()| code));
    match result {
        Ok(code) => ExitCode::from(code),
        Err(failure) => {
            eprintln!("npcctl: {}", failure.message());
            let text = if cli.pretty {
                serde_json::to_string_pretty(&failure.body)
            } else {
                serde_json::to_string(&failure.body)
            };
            println!("{}", text.unwrap_or_default());
            ExitCode::from(failure.code)
        }
    }
}

type CmdResult = Result<u8, Failure>;

/// The content's end-of-run summary (`None` for the village).
fn content_summary(scenario: &Scenario, world: &bevy_ecs::world::World) -> Option<Value> {
    match scenario.content() {
        "testbed" => Some(testbed_content::summary::summary_json(world)),
        _ => None,
    }
}

fn cmd_run(a: &RunArgs, out: &mut Out, pretty: bool) -> CmdResult {
    let scenario = a.world.scenario()?;
    let commands = load_optional_commands(a.world.commands.as_deref())?;
    let mut driver = Driver::new(scenario.build()?, commands);
    if let Some(path) = &a.journal {
        let sink = JournalSink::create(path)
            .map_err(|e| Failure::usage(format!("cannot create {}: {e}", path.display())))?;
        let world = driver.world();
        set_journal(world, true, a.verbose);
        let mut journal = world.resource_mut::<Journal>();
        // Only the file is needed: keep no entries in memory.
        journal.capacity = 0;
        journal.sink = Some(sink);
    }
    let mut recorder = a
        .record
        .as_ref()
        .map(|_| recorder(a.checkpoint_every, a.ticks));
    let village = scenario.content() == "village";
    let mut timeline = Timeline::new(driver.world());
    if !village {
        timeline = timeline.with_other_brains(driver.world());
    }
    if a.all_needs {
        timeline = timeline.with_all_needs(driver.world());
    }
    while driver.tick() < a.ticks {
        let tick = driver.tick();
        driver.step()?;
        let world = driver.world();
        timeline.observe(world, tick);
        if let Some(r) = &mut recorder {
            r.after_tick(world);
        }
    }
    let world = driver.world();
    world.resource_mut::<Journal>().flush();
    let final_hash = npc_framework::kernel::hash::state_hash(world);
    if let (Some(r), Some(path)) = (recorder, &a.record) {
        let replay = finish_replay(r, &scenario, driver.world())?;
        write_json_file(path, &replay, pretty)?;
    }
    let config = driver
        .app
        .world()
        .get_resource::<npc_framework::prelude::SimConfig>()
        .copied()
        .unwrap_or_default();
    let aborts: u32 = timeline.days.iter().map(|d| d.aborts).sum();
    let leaf_starts: u32 = timeline.days.iter().map(|d| d.leaf_starts).sum();
    let villagers = if village {
        scenario.villagers
    } else {
        units_of(driver.app.world()).unwrap_or(scenario.villagers)
    };
    out.json(&RunSummary {
        seed: scenario.seed,
        scenario: scenario.name.clone(),
        villagers,
        ticks: a.ticks,
        day_length: config.day_length,
        final_tick: driver.tick(),
        final_hash,
        days: &timeline.days,
        needs: timeline.need_ranges(),
        needs_all: timeline.all_need_ranges(),
        aborts,
        leaf_starts,
        commands_applied: driver.commands_applied(),
        commands_not_reached: driver.not_reached(),
        journal: a.journal.as_ref().map(|p| p.display().to_string()),
        record: a.record.as_ref().map(|p| p.display().to_string()),
        content_summary: content_summary(&scenario, driver.app.world()),
    })?;
    Ok(EXIT_OK)
}

fn cmd_inspect(a: &InspectArgs, out: &mut Out) -> CmdResult {
    let commands = load_optional_commands(a.world.commands.as_deref())?;
    let scenario = a.world.scenario()?;
    let app = if a.journal_from_build {
        scenario.build_journaled(false)?
    } else {
        scenario.build()?
    };
    let mut driver = Driver::new(app, commands);
    // The journal never changes sim results; it fills `recent_journal`.
    set_journal(driver.world(), true, false);
    driver.run_to(a.tick)?;
    let world = driver.app.world();
    let unknown = |id: u32| {
        Failure::usage(format!("unknown unit {id} at tick {}", a.tick))
            .with("reason", CommandError::UnknownUnit { unit: id })
    };
    if scenario.content() == "village" {
        match a.unit {
            Some(id) => {
                let unit = inspect::unit(world, SimId(id)).ok_or_else(|| unknown(id))?;
                out.json(&unit)?;
            }
            None => out.json(&inspect::snapshot(world))?,
        }
    } else {
        // Other contents: the kernel's installed inspector (generic unless
        // the content sets one), every registered component by name.
        match a.unit {
            Some(id) => {
                let unit =
                    kernel_inspect::unit_json(world, SimId(id)).ok_or_else(|| unknown(id))?;
                out.json(&unit)?;
            }
            None => out.json(&kernel_inspect::world_json(world))?,
        }
    }
    Ok(EXIT_OK)
}

fn cmd_journal(a: &JournalArgs, out: &mut Out) -> CmdResult {
    if let Some(bad) = a.kind.iter().find(|k| JournalKind::parse_name(k).is_none()) {
        return Err(Failure::usage(format!(
            "unknown journal kind `{bad}` (expected one of {})",
            JournalKind::NAMES.join(", ")
        ))
        .with("kinds", &JournalKind::NAMES[..]));
    }
    let filter = JournalFilter {
        unit: a.unit.map(SimId),
        kinds: a.kind.clone(),
        from: a.from,
        to: a.to,
        grep: a.grep.clone(),
        limit: None,
    };
    let commands = load_optional_commands(a.world.commands.as_deref())?;
    let scenario = a.world.scenario()?;
    let app = if a.journal_from_build {
        scenario.build_journaled(a.verbose)?
    } else {
        scenario.build()?
    };
    let mut driver = Driver::new(app, commands);
    set_journal(driver.world(), true, a.verbose);
    let end = a.to.map_or(a.ticks, |to| a.ticks.min(to.saturating_add(1)));
    let mut kept = VecDeque::new();
    while driver.tick() < end {
        let tick = driver.tick();
        driver.step()?;
        let journal = driver.app.world().resource::<Journal>();
        // This tick's entries are the newest ones in the ring buffer (on the
        // first tick they include the build-time tick-0 Spawn entries).
        let new = journal
            .entries
            .iter()
            .rev()
            .take_while(|e| e.tick == tick)
            .count();
        let start = journal.entries.len() - new;
        for entry in journal.entries.range(start..).filter(|e| filter.matches(e)) {
            match a.limit {
                Some(limit) => {
                    if kept.len() == limit {
                        kept.pop_front();
                    }
                    if limit > 0 {
                        kept.push_back(entry.clone());
                    }
                }
                None => out.line(entry)?,
            }
        }
    }
    for entry in &kept {
        out.line(entry)?;
    }
    Ok(EXIT_OK)
}

/// A replay is verified by the factory of its scenario's content (the
/// village for `village*`, the testbed for `testbed[:<name>]`); a session
/// record (`serve --record`, or `replay record --scenario FILE`) from its
/// scenario document, as `replay verify-session` does.
fn cmd_verify(file: &Path, out: &mut Out) -> CmdResult {
    let doc: Value = read_json(file, "replay")?;
    if is_session_record(&doc) {
        return agent::cmd_verify_session(file, out);
    }
    let replay: Replay = read_json(file, "replay")?;
    let report = verify_replay(&replay);
    out.json(&report)?;
    Ok(if report.ok {
        EXIT_OK
    } else {
        EXIT_CHECK_FAILED
    })
}

fn cmd_record(a: &RecordArgs, out: &mut Out) -> CmdResult {
    let scenario = a.world.scenario()?;
    let commands = load_optional_commands(a.world.commands.as_deref())?;
    let mut driver = Driver::new(scenario.build()?, commands);
    let mut recorder = recorder(a.checkpoint_every, a.ticks);
    while driver.tick() < a.ticks {
        driver.step()?;
        recorder.after_tick(driver.world());
    }
    match finish_replay(recorder, &scenario, driver.world())? {
        Recorded::Replay(replay) => out.json(&replay)?,
        Recorded::Session(record) => out.json(&record)?,
    }
    Ok(EXIT_OK)
}

fn cmd_snapshot_save(
    world: &WorldArgs,
    ticks: u64,
    file: Option<&Path>,
    out: &mut Out,
) -> CmdResult {
    use npc_framework::snapshot;
    let file = file.ok_or_else(|| Failure::usage("snapshot save needs --out FILE"))?;
    let scenario = world.scenario()?;
    let commands = load_optional_commands(world.commands.as_deref())?;
    let mut driver = Driver::new(scenario.build()?, commands);
    while driver.tick() < ticks {
        driver.step()?;
    }
    let snap = snapshot::save(driver.world()).map_err(Failure::usage)?;
    snapshot::write_atomic(file, &snap)
        .map_err(|e| Failure::usage(format!("cannot write {}: {e}", file.display())))?;
    out.json(&serde_json::json!({
        "ok": true,
        "file": file.display().to_string(),
        "scenario": scenario.name,
        "seed": scenario.seed,
        "tick": snap.tick,
        "entities": snap.entities.len(),
    }))?;
    Ok(EXIT_OK)
}

fn cmd_snapshot_check(world: &WorldArgs, file: &Path, out: &mut Out) -> CmdResult {
    use npc_framework::snapshot;
    let snap = snapshot::read(file).map_err(Failure::usage)?;
    let scenario = world.scenario()?;
    let mut app = scenario.build()?;
    match snapshot::check(app.world_mut(), &snap) {
        Ok(()) => {
            out.json(&serde_json::json!({ "ok": true }))?;
            Ok(EXIT_OK)
        }
        Err(e) => {
            out.json(&serde_json::json!({ "ok": false, "error": e.to_string() }))?;
            Ok(EXIT_CHECK_FAILED)
        }
    }
}

fn cmd_diverge(a: &DivergeArgs, out: &mut Out) -> CmdResult {
    let (mut ra, mut rb, factory) = match (&a.a, &a.b, a.seed_a, a.seed_b) {
        (Some(fa), Some(fb), _, _) => {
            let (ra, rb) = (
                read_json::<Replay>(fa, "replay")?,
                read_json::<Replay>(fb, "replay")?,
            );
            let (ca, cb) = (content_of(&ra.scenario), content_of(&rb.scenario));
            if ca.name != cb.name {
                return Err(Failure::usage(format!(
                    "the replays are of different contents ({} and {})",
                    ca.name, cb.name
                )));
            }
            (ra, rb, ca.factory)
        }
        (_, _, Some(sa), Some(sb)) => {
            let commands = load_optional_commands(a.commands.as_deref())?;
            let ticks = a.ticks.unwrap_or(2400);
            let make = |seed| Scenario::new(Some(seed), a.scenario.as_deref(), a.villagers);
            let (sa, sb) = (make(sa)?, make(sb)?);
            let factory = sa.factory("diverge")?;
            (
                sa.replay(ticks, &commands),
                sb.replay(ticks, &commands),
                factory,
            )
        }
        _ => {
            return Err(Failure::usage(
                "give --a FILE --b FILE or --seed-a N --seed-b M",
            ));
        }
    };
    if let Some(t) = a.ticks {
        ra.ticks = ra.ticks.min(t);
        rb.ticks = rb.ticks.min(t);
    }
    let compared = ra.ticks.min(rb.ticks);
    let divergence =
        npc_framework::kernel::replay::try_diverge(&ra, &rb, factory).map_err(Failure::usage)?;
    let (tick, entities) = match divergence {
        Some(d) => (Some(d.tick), d.entities),
        None => (None, Vec::new()),
    };
    let shown: Vec<DivergeEntity> = entities
        .iter()
        .take(a.max_entities)
        .map(|e| DivergeEntity {
            sim_id: e.sim_id,
            components_differing: &e.components,
            a: &e.a,
            b: &e.b,
        })
        .collect();
    out.json(&DivergeOutput {
        first_divergent_tick: tick,
        ticks_compared: compared,
        entities_differing: entities.len(),
        truncated: entities.len() > shown.len(),
        entities: shown,
    })?;
    Ok(if tick.is_some() {
        EXIT_CHECK_FAILED
    } else {
        EXIT_OK
    })
}

fn cmd_bench(a: &BenchArgs, collector: &PerfCollector, out: &mut Out) -> CmdResult {
    if a.villagers.is_empty() {
        return Err(Failure::usage("--villagers needs at least one size"));
    }
    let scenario = Scenario::new(Some(a.seed), Some(&a.scenario), 1)?;
    let factory = scenario.factory("bench")?;
    let sized = matches!(scenario.source, sim::Source::Named(c) if c.sized);
    // A fixed-size scenario is measured once, at its own unit count.
    let fixed = if sized {
        Vec::new()
    } else {
        let app = factory(a.seed, &scenario.name, 1).map_err(Failure::usage)?;
        vec![units_of(app.world()).unwrap_or(1)]
    };
    let mut report = bench::run(&bench::BenchArgs {
        collector,
        factory,
        scenario: &scenario.name,
        villagers: if sized { &a.villagers } else { &fixed },
        ticks: a.ticks,
        warmup: a.warmup,
        seed: a.seed,
        top: a.top,
    })?;
    let mut code = EXIT_OK;
    if let Some(path) = &a.baseline {
        let comparison = bench::compare(
            &report,
            path,
            a.metric,
            a.threshold_pct,
            a.min_us,
            a.min_samples,
        )?;
        for r in &comparison.regressions {
            eprintln!(
                "regression: {} villagers, {}: {:?} {:.2} us vs baseline {:.2} us (+{:.0}%)",
                r.villagers, r.mechanic, a.metric, r.value_us, r.baseline_us, r.change_pct
            );
        }
        if !comparison.regressions.is_empty() {
            code = EXIT_CHECK_FAILED;
        }
        report.comparison = Some(comparison);
    }
    out.json(&report)?;
    Ok(code)
}
