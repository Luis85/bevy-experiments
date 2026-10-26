//! `save` (run to a tick and write `save_world`) and `load` (continue a save).

use std::path::Path;

use fl_sim::{SimIndex, compute_state_hash, save_world};
use serde_json::json;

use crate::agent::{Game, ScenarioOptions, Session, SessionConfig, Source};
use crate::report::{CliError, Report, hash_hex};

/// `save`: run `source` until the clock reads `tick` and write [`save_world`] to `out`, with
/// the commands the driver still holds (a replay's remaining inputs, so a load of the save
/// plays the rest of the replay).
///
/// JSON: `{"out", "tick", "hash", "entities", "pending_commands"}`; `hash` is the state hash
/// at `tick`.
///
/// # Errors
/// As [`Session::open`] and [`Session::run_to`]; [`CliError::Io`] if `out` cannot be written.
pub fn save(
    game: &Game,
    source: &Source,
    options: &ScenarioOptions,
    tick: u32,
    out: &Path,
) -> Result<Report, CliError> {
    let mut session = Session::open(game, source, options, SessionConfig::default())?;
    session.run_to(tick)?;
    let pending = session.pending();
    let world = session.world();
    let bytes = save_world(world, &pending).map_err(|e| CliError::Setup(e.to_string()))?;
    std::fs::write(out, &bytes)
        .map_err(|e| CliError::Io(format!("cannot write {}: {e}", out.display())))?;
    let hash = hash_hex(compute_state_hash(world));
    let entities = world.resource::<SimIndex>().len();
    let json = json!({
        "out": out, "tick": tick, "hash": hash, "entities": entities,
        "pending_commands": pending.len(),
    });
    let text = format!(
        "saved tick {tick} to {} ({} bytes)\nhash: {hash}\nentities: {entities}\n\
         pending commands: {}\n",
        out.display(),
        bytes.len(),
        pending.len()
    );
    Ok(Report::ok(json, text))
}

/// `load`: load the save at `path`, run `ticks` ticks with its pending commands, and print
/// the final state hash.
///
/// JSON: `{"start_tick", "ticks", "tick", "final_hash"}`; `tick` is the clock at the end.
///
/// # Errors
/// As [`Session::open`].
pub fn load(game: &Game, path: &Path, ticks: u32) -> Result<Report, CliError> {
    let source = Source::Save(path.to_owned());
    let mut session = Session::open(
        game,
        &source,
        &ScenarioOptions::default(),
        SessionConfig::default(),
    )?;
    let start = session.next_tick().0;
    for _ in 0..ticks {
        session.step()?;
    }
    let tick = session.next_tick().0;
    let final_hash = hash_hex(compute_state_hash(session.world()));
    let json = json!({
        "start_tick": start, "ticks": ticks, "tick": tick, "final_hash": final_hash,
    });
    let text = format!(
        "loaded tick {start}, ran {ticks} ticks to tick {tick}\nfinal hash: {final_hash}\n"
    );
    Ok(Report::ok(json, text))
}
