//! The `fl_cli` binary.
//!
//! Exit codes: 0 success; 1 failure (an error, a failed check, a divergence); 2 bad usage;
//! 3 a non-reproducible replay (`replay-verify`). With `--json`, stdout holds exactly one JSON
//! document, an error included (`{"error": "..."}`); logs go to stderr.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::error::ErrorKind;
use clap::{Args, Parser, Subcommand};
use fl_cli::{
    BenchConfig, BenchOutputs, CliError, Game, JournalFilter, MapPreset, ReblessArgs, Report,
    RunScenario, ScenarioOptions, Source, bench_report, bisect_desync, data_check_report,
    hash_dump_report, init_logs, inspect, journal, load, map_gen, query, rebless, replay_dump,
    replay_verify, run_scenario, save,
};

#[derive(Parser)]
#[command(name = "fl_cli", version, about = "Frontline headless tools")]
struct Cli {
    /// Print exactly one JSON document on stdout (logs go to stderr).
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    command: Command,
}

/// The content to run with.
#[derive(Args)]
struct ModsArg {
    /// Mod directories, in load order (default: the base mod).
    #[arg(long, num_args = 1..)]
    mods: Vec<PathBuf>,
}

/// How a scenario is set up (scenario sources only).
#[derive(Args)]
struct ScenarioArgs {
    /// The match seed instead of the scenario's.
    #[arg(long)]
    seed: Option<u64>,
    /// Before tick 0, every player orders all its units to wander this many metres
    /// (`debug.wander`; the match allows cheats).
    #[arg(long)]
    wander: Option<i32>,
}

impl ScenarioArgs {
    fn options(&self) -> ScenarioOptions {
        ScenarioOptions {
            seed: self.seed,
            wander: self.wander,
        }
    }
}

#[derive(Subcommand)]
enum Command {
    /// Validate mods; prints every error and exits 1 on failure.
    DataCheck {
        /// Mod directories, in load order.
        #[arg(required = true)]
        dirs: Vec<PathBuf>,
    },
    /// Run a scenario headless and print `tick hash` lines.
    HashDump {
        /// Mod directories, in load order.
        #[arg(long, required = true, num_args = 1..)]
        mods: Vec<PathBuf>,
        /// The scenario id.
        #[arg(long)]
        scenario: String,
        /// How many ticks to run.
        #[arg(long)]
        ticks: u32,
        /// Hash every this many ticks.
        #[arg(long, default_value_t = 20)]
        interval: u32,
    },
    /// Wall-clock bench of the sim at scale (informational; median of `--repeat` runs).
    Bench {
        /// Unit counts, comma separated.
        #[arg(long, value_delimiter = ',', default_values_t = [500, 3000, 5000])]
        units: Vec<u32>,
        /// Measured ticks per run.
        #[arg(long, default_value_t = 400)]
        ticks: u32,
        /// Unmeasured ticks before each run.
        #[arg(long, default_value_t = 40)]
        warmup: u32,
        /// The match seed.
        #[arg(long, default_value_t = 1)]
        seed: u64,
        /// Runs per size; the median by mean tick time is reported (the upper middle run for
        /// an even count; 0 counts as 1).
        #[arg(long, default_value_t = 5)]
        repeat: u32,
        /// Run on M0's flat, open map instead of the 2 km terrain map (the workload
        /// `docs/perf/M0.json` measured).
        #[arg(long)]
        flat: bool,
        /// Also write the markdown table here.
        #[arg(long)]
        out: Option<PathBuf>,
        /// Write the results as JSON here (e.g. `docs/perf/<milestone>.json`).
        #[arg(long)]
        json_out: Option<PathBuf>,
        /// Include the profiler's per-size phase, feature and system breakdown in the JSON
        /// (needs `--json` or `--json-out`).
        #[arg(long)]
        profile: bool,
        /// A JSON file written by an earlier `--json-out`; exit 1 on a regression against it.
        #[arg(long)]
        baseline: Option<PathBuf>,
        /// The allowed slowdown of the mean tick time against the baseline (0.20 = 20 %).
        #[arg(long, default_value_t = 0.20)]
        max_regression: f64,
        /// Compare against a `--baseline` that ran on another map instead of refusing.
        #[arg(long)]
        force: bool,
    },
    /// Run a scenario headless into a new run directory (log.jsonl, journal.jsonl, run.replay,
    /// states.jsonl).
    RunScenario {
        /// A scenario id or a `*.scenario.ron` path.
        scenario: String,
        /// How many ticks to run.
        #[arg(long)]
        ticks: u32,
        #[command(flatten)]
        setup: ScenarioArgs,
        /// Log everything (`trace`) instead of `info,fl_sim=debug`.
        #[arg(long)]
        trace: bool,
        /// Write the world and every entity to states.jsonl every this many ticks.
        #[arg(long)]
        dump_state_every: Option<u32>,
        /// The run directory `<run id>/` is created in here.
        #[arg(long, default_value = "logs")]
        out: PathBuf,
        #[command(flatten)]
        mods: ModsArg,
    },
    /// Re-simulate to a tick and print the world summary, or one entity.
    Inspect {
        /// A `*.replay`, a run directory, a `*.save`, a scenario id or a `*.scenario.ron`.
        source: String,
        /// The tick: the state after ticks 0..T ran.
        #[arg(long)]
        tick: u32,
        /// Print this entity (a sim id) instead of the world.
        #[arg(long)]
        entity: Option<u32>,
        #[command(flatten)]
        setup: ScenarioArgs,
        #[command(flatten)]
        mods: ModsArg,
    },
    /// Re-simulate to a tick and print the answer of a named debug query
    /// (`core.debug_queries` lists them).
    Query {
        /// A `*.replay`, a run directory, a `*.save`, a scenario id or a `*.scenario.ron`.
        source: String,
        /// The tick: the state after ticks 0..T ran.
        #[arg(long)]
        tick: u32,
        /// The query's name, such as `core.debug_queries`.
        #[arg(long)]
        name: String,
        /// The query's arguments, as JSON (default: null).
        #[arg(long)]
        args: Option<String>,
        #[command(flatten)]
        setup: ScenarioArgs,
        #[command(flatten)]
        mods: ModsArg,
    },
    /// Print the journal entries of a run that match every filter, in tick order.
    Journal {
        /// A run directory or a journal.jsonl.
        path: PathBuf,
        /// The record kind, such as `core.unit_spawned`.
        #[arg(long)]
        kind: Option<String>,
        /// The entity's sim id.
        #[arg(long)]
        sim_id: Option<u32>,
        /// The player id.
        #[arg(long)]
        player: Option<u8>,
        /// The first tick (inclusive).
        #[arg(long)]
        from: Option<u32>,
        /// The last tick (inclusive).
        #[arg(long)]
        to: Option<u32>,
        /// At most this many entries.
        #[arg(long)]
        limit: Option<usize>,
    },
    /// Play a replay back and compare its hashes: exit 0 verified, 1 diverged, 3
    /// non-reproducible.
    ReplayVerify {
        /// The `*.replay` file.
        file: PathBuf,
        #[command(flatten)]
        mods: ModsArg,
    },
    /// Replay tools.
    #[command(subcommand)]
    Replay(ReplayCommand),
    /// Re-simulate to a tick and write a save.
    Save {
        /// A scenario id, a `*.scenario.ron`, a `*.replay`, a run directory or a `*.save`.
        source: String,
        /// The tick to save at: after ticks 0..T ran.
        #[arg(long)]
        tick: u32,
        /// The save file to write.
        #[arg(long)]
        out: PathBuf,
        #[command(flatten)]
        setup: ScenarioArgs,
        #[command(flatten)]
        mods: ModsArg,
    },
    /// Load a save, run on and print the final state hash.
    Load {
        /// The `*.save` file.
        file: PathBuf,
        /// How many ticks to run after the save's tick.
        #[arg(long)]
        ticks: u32,
        #[command(flatten)]
        mods: ModsArg,
    },
    /// Write a base map's procedural height and terrain PNG layers.
    MapGen {
        /// Which map.
        preset: MapPreset,
        /// The directory the two PNGs are written into.
        #[arg(long)]
        out: PathBuf,
    },
    /// Re-simulate two replays (or run directories) with a hash every tick and report the
    /// first tick at which they differ: exit 0 identical, 1 diverged.
    BisectDesync {
        a: PathBuf,
        b: PathBuf,
        #[command(flatten)]
        mods: ModsArg,
    },
    /// Re-run every golden replay of a directory on this build under a mechanic change
    /// request: write `<name>.replay.new` for each whose bytes change, and a report. Exit 1,
    /// writing nothing, if the changelog has no `## MCR-nnn` heading or a semantic check fails
    /// (ADR-0021; the lead blesses the `.new` files, R146).
    Rebless {
        /// The MCR id, such as `MCR-001`.
        #[arg(long)]
        reason: String,
        /// The directory of the `*.replay` files.
        #[arg(long, default_value = "crates/fl_test_utils/tests/replays")]
        dir: PathBuf,
        /// The MCR changelog.
        #[arg(long, default_value = "docs/design/changelog-mechanics.md")]
        changelog: PathBuf,
        /// The markdown report.
        #[arg(long, default_value = "target/rebless-report.md")]
        report: PathBuf,
        #[command(flatten)]
        mods: ModsArg,
    },
}

#[derive(Subcommand)]
enum ReplayCommand {
    /// Print a replay: the whole replay as JSON (`--json`) or a summary.
    Dump {
        /// The `*.replay` file.
        file: PathBuf,
    },
}

fn main() -> ExitCode {
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(err) => return clap_error(&err),
    };
    let json = cli.json;
    match run(cli.command, json) {
        Ok(report) => {
            if json {
                println!("{:#}", report.json);
            } else {
                print!("{}", report.text);
            }
            ExitCode::from(report.code)
        }
        Err(err) => {
            eprintln!("error: {err}");
            if json {
                println!("{:#}", serde_json::json!({ "error": err.to_string() }));
            }
            ExitCode::from(err.exit_code())
        }
    }
}

/// Print what clap has to say: as clap does without `--json`; with it, as one JSON document
/// (`{"help"}`, `{"version"}` or `{"error"}`, the error also on stderr), with clap's exit code.
fn clap_error(err: &clap::Error) -> ExitCode {
    if !std::env::args_os().skip(1).any(|arg| arg == "--json") {
        err.exit();
    }
    let text = err.render().to_string();
    let doc = match err.kind() {
        ErrorKind::DisplayHelp => serde_json::json!({ "help": text }),
        ErrorKind::DisplayVersion => serde_json::json!({ "version": text.trim() }),
        _ => {
            eprint!("{text}");
            serde_json::json!({ "error": text })
        }
    };
    println!("{doc:#}");
    ExitCode::from(u8::try_from(err.exit_code()).unwrap_or(2))
}

fn run(command: Command, json: bool) -> Result<Report, CliError> {
    // run-scenario logs into its run directory; bench installs the profiler.
    if !matches!(command, Command::RunScenario { .. } | Command::Bench { .. }) {
        init_logs("warn", None);
    }
    match command {
        Command::DataCheck { dirs } => Ok(data_check_report(&dirs)),
        Command::HashDump {
            mods,
            scenario,
            ticks,
            interval,
        } => hash_dump_report(&mods, &scenario, ticks, interval),
        Command::Bench {
            units,
            ticks,
            warmup,
            seed,
            repeat,
            flat,
            out,
            json_out,
            profile,
            baseline,
            max_regression,
            force,
        } => {
            if profile && !json && json_out.is_none() {
                return Err(CliError::Usage(
                    "--profile needs --json or --json-out".to_owned(),
                ));
            }
            let cfg = BenchConfig {
                units,
                ticks,
                warmup,
                seed,
                repeat,
                flat,
            };
            let outputs = BenchOutputs {
                markdown: out,
                json: json_out,
                profile,
                baseline,
                max_regression,
                force,
            };
            bench_report(&cfg, &outputs)
        }
        Command::RunScenario {
            scenario,
            ticks,
            setup,
            trace,
            dump_state_every,
            out,
            mods,
        } => {
            let game = Game::load(&mods.mods)?;
            let args = RunScenario {
                scenario,
                options: setup.options(),
                ticks,
                trace,
                dump_state_every,
                out,
            };
            run_scenario(&game, &args)
        }
        Command::Inspect {
            source,
            tick,
            entity,
            setup,
            mods,
        } => inspect(
            &Game::load(&mods.mods)?,
            &Source::parse(&source),
            &setup.options(),
            tick,
            entity,
        ),
        Command::Query {
            source,
            tick,
            name,
            args,
            setup,
            mods,
        } => query(
            &Game::load(&mods.mods)?,
            &Source::parse(&source),
            &setup.options(),
            tick,
            &name,
            args.as_deref(),
        ),
        Command::Journal {
            path,
            kind,
            sim_id,
            player,
            from,
            to,
            limit,
        } => {
            let filter = JournalFilter {
                kind,
                sim_id,
                player,
                from,
                to,
                limit,
            };
            journal(&path, &filter)
        }
        Command::ReplayVerify { file, mods } => replay_verify(&Game::load(&mods.mods)?, &file),
        Command::Replay(ReplayCommand::Dump { file }) => replay_dump(&file),
        Command::Save {
            source,
            tick,
            out,
            setup,
            mods,
        } => save(
            &Game::load(&mods.mods)?,
            &Source::parse(&source),
            &setup.options(),
            tick,
            &out,
        ),
        Command::Load { file, ticks, mods } => load(&Game::load(&mods.mods)?, &file, ticks),
        Command::MapGen { preset, out } => map_gen(preset, &out),
        Command::BisectDesync { a, b, mods } => bisect_desync(&Game::load(&mods.mods)?, &a, &b),
        Command::Rebless {
            reason,
            dir,
            changelog,
            report,
            mods,
        } => rebless(
            &Game::load(&mods.mods)?,
            &ReblessArgs {
                reason,
                dir,
                changelog,
                report,
            },
        ),
    }
}
