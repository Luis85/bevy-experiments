//! `--commands` files: a JSON array of `{"tick": T, "cmd": {..}}` items or
//! replay `[tick, seq, {..}]` tuples, or a whole replay file.

use std::collections::BTreeMap;
use std::path::Path;

use npc_kernel::SimCommand;
use serde_json::Value;

use crate::output::{Failure, read_json};

/// One command from a `--commands` file.
#[derive(Clone, Debug)]
pub struct TimedCommand {
    pub tick: u64,
    /// Order within the tick (the replay seq, or the file order).
    pub seq: u32,
    /// Position in the file, for error messages.
    pub index: usize,
    pub cmd: SimCommand,
}

/// Loads a `--commands` file: a JSON array whose items are either
/// `{"tick": T, "cmd": {..}}` or replay tuples `[tick, seq, {..}]`, or a
/// whole replay file (its `commands` are used). Sorted by `(tick, seq)`;
/// `{tick, cmd}` items get seq = their order within the tick in the file.
pub fn load_commands(path: &Path) -> Result<Vec<TimedCommand>, Failure> {
    let doc: Value = read_json(path, "commands file")?;
    let items = match doc {
        Value::Array(items) => items,
        Value::Object(mut map) => match map.remove("commands") {
            Some(Value::Array(items)) => items,
            _ => {
                return Err(bad_commands(
                    path,
                    "expected a JSON array (or a replay object)",
                ));
            }
        },
        _ => return Err(bad_commands(path, "expected a JSON array")),
    };
    let mut per_tick: BTreeMap<u64, u32> = BTreeMap::new();
    let mut out = Vec::with_capacity(items.len());
    for (index, item) in items.into_iter().enumerate() {
        let parsed = match item {
            Value::Object(mut map) => {
                let tick = map.remove("tick").and_then(|t| t.as_u64());
                let cmd = map.remove("cmd");
                match (tick, cmd) {
                    (Some(tick), Some(cmd)) => serde_json::from_value::<SimCommand>(cmd)
                        .map(|cmd| (tick, None, cmd))
                        .map_err(|e| e.to_string()),
                    _ => Err("expected {\"tick\": <u64>, \"cmd\": {..}}".to_owned()),
                }
            }
            Value::Array(_) => serde_json::from_value::<(u64, u32, SimCommand)>(item)
                .map(|(t, s, c)| (t, Some(s), c))
                .map_err(|e| format!("expected [tick, seq, cmd]: {e}")),
            _ => Err("expected an object or an array".to_owned()),
        };
        let (tick, seq, cmd) = parsed
            .map_err(|e| bad_commands(path, &format!("item #{index}: {e}")).with("index", index))?;
        let seq = seq.unwrap_or_else(|| {
            let next = per_tick.entry(tick).or_insert(0);
            *next += 1;
            *next - 1
        });
        out.push(TimedCommand {
            tick,
            seq,
            index,
            cmd,
        });
    }
    out.sort_by_key(|c| (c.tick, c.seq));
    Ok(out)
}

fn bad_commands(path: &Path, msg: &str) -> Failure {
    Failure::usage(format!("commands file {}: {msg}", path.display()))
        .with("path", path.display().to_string())
}

pub fn load_optional_commands(path: Option<&Path>) -> Result<Vec<TimedCommand>, Failure> {
    path.map_or(Ok(Vec::new()), load_commands)
}
