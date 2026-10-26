//! `query`: a named debug query (ADR-0015) at a tick.

use fl_sim::debug_query;
use serde_json::Value;

use crate::agent::{Game, ScenarioOptions, Session, SessionConfig, Source};
use crate::report::{CliError, Report};

/// `query`: re-simulate `source` to `tick` as [`inspect`](crate::inspect) does, then answer
/// the debug query `name` with `args` (JSON; `null` when absent) and print its answer.
///
/// # Errors
/// [`CliError::Usage`] if `args` is not JSON or the tick is out of range; [`CliError::Check`]
/// with `"unknown debug query '<name>'"` or the query's own message; any error setting the
/// source up.
pub fn query(
    game: &Game,
    source: &Source,
    options: &ScenarioOptions,
    tick: u32,
    name: &str,
    args: Option<&str>,
) -> Result<Report, CliError> {
    let args: Value = match args {
        Some(text) => serde_json::from_str(text)
            .map_err(|err| CliError::Usage(format!("--args is not JSON: {err}")))?,
        None => Value::Null,
    };
    let mut session = Session::open(game, source, options, SessionConfig::default())?;
    session.run_to(tick)?;
    let answer = debug_query(session.world(), name, &args).map_err(CliError::Check)?;
    let text = format!("{answer:#}\n");
    Ok(Report::ok(answer, text))
}
