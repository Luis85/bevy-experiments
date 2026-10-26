//! fl_cli: headless tools for the content and the sim (spec §15 C): `data-check`,
//! `hash-dump`, `bench`, and the agent debugging commands `run-scenario`, `inspect`, `query`,
//! `journal`, `map-gen`, `replay-verify`, `replay dump`, `save`, `load` and `bisect-desync`, and
//! `rebless` (golden replays re-run under an MCR, ADR-0021). With the global `--json` flag every
//! command prints exactly one JSON document on stdout; logs go to stderr.

mod agent;
mod bench;
mod data_check;
mod hash_dump;
mod inspect;
mod journal_cmd;
mod map_gen;
mod query_cmd;
mod rebless;
mod report;
mod save_cmd;

pub use agent::{
    Game, RunScenario, ScenarioOptions, Session, SessionConfig, Source, init_logs, read_replay,
    replay_dump, replay_verify, run_scenario,
};
pub use bench::{
    BenchConfig, BenchOutputs, BenchRow, CompareError, bench_report, compare, exclusive, median,
    profile_json, render_markdown, rows_from_json, run_bench, run_bench_full,
};
pub use data_check::{DataCheckReport, data_check, data_check_report};
pub use hash_dump::{hash_dump, hash_dump_report};
pub use inspect::{bisect_desync, inspect};
pub use journal_cmd::{JournalFilter, journal, read_journal};
pub use map_gen::{MapPreset, generate, map_gen};
pub use query_cmd::query;
pub use rebless::{ReblessArgs, rebless};
pub use report::{CliError, Report, hash_hex};
pub use save_cmd::{load, save};
