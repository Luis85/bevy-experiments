//! `hash-dump`: run a scenario headless and list its state hashes.

use std::path::PathBuf;

use fl_protocol::Tick;
use fl_sim::{MatchSetup, RunConfig, build_sim_app, load_content, setup_scenario, step_tick};
use fl_view::TickInput;
use serde_json::json;

use crate::report::{CliError, Report};

/// Run `scenario` of `mods` for `ticks` ticks without commands and return the state hash
/// every `interval` ticks, starting at tick 0.
///
/// # Errors
/// [`CliError::Usage`] if `interval` is 0, [`CliError::Build`] or [`CliError::Setup`].
pub fn hash_dump(
    mods: &[PathBuf],
    scenario: &str,
    ticks: u32,
    interval: u32,
) -> Result<Vec<(Tick, u64)>, CliError> {
    if interval == 0 {
        return Err(CliError::Usage("--interval must be at least 1".to_owned()));
    }
    let features = fl_catalog::features();
    let build = |e: fl_sim::BuildError| CliError::Build(e.to_string());
    let content = load_content(&features, mods).map_err(build)?;
    let setup =
        MatchSetup::for_scenario(&content, scenario).map_err(|e| CliError::Setup(e.to_string()))?;
    let run = RunConfig {
        hash_interval: Some(interval),
        ..RunConfig::default()
    };
    let mut app = build_sim_app(&features, content, setup, run).map_err(build)?;
    let world = app.world_mut();
    setup_scenario(world, scenario).map_err(|e| CliError::Setup(e.to_string()))?;
    let mut hashes = Vec::new();
    for tick in 0..ticks {
        let input = TickInput {
            tick: Tick(tick),
            commands: Vec::new(),
        };
        let out = step_tick(world, input);
        if let Some(hash) = out.hash {
            hashes.push((out.tick, hash));
        }
    }
    Ok(hashes)
}

/// `hash-dump`: [`hash_dump`] as `tick hash` lines (text) or `[{"tick", "hash"}]` (JSON).
///
/// # Errors
/// As [`hash_dump`].
pub fn hash_dump_report(
    mods: &[PathBuf],
    scenario: &str,
    ticks: u32,
    interval: u32,
) -> Result<Report, CliError> {
    let hashes = hash_dump(mods, scenario, ticks, interval)?;
    let rows: Vec<_> = hashes
        .iter()
        .map(|(t, h)| json!({"tick": t.0, "hash": format!("0x{h:016x}")}))
        .collect();
    let text: String = hashes
        .iter()
        .map(|(tick, hash)| format!("{} {hash:016x}\n", tick.0))
        .collect();
    Ok(Report::ok(serde_json::Value::Array(rows), text))
}
