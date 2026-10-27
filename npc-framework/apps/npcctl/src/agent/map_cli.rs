//! Map clients share the agent transport and optimistic stamp conventions.
use clap::{Args, Subcommand, ValueEnum};
use serde_json::{Value, json};

use super::{ClientArgs, Failure, Out, client, exit_code, print, rpc};

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum MapWriter {
    Player,
}

fn position_arg(text: &str) -> Result<[i32; 2], String> {
    let (x, z) = text
        .split_once(',')
        .ok_or("position must be x,z in millimetres")?;
    Ok([
        x.trim()
            .parse()
            .map_err(|_| "x must be a signed 32-bit integer")?,
        z.trim()
            .parse()
            .map_err(|_| "z must be a signed 32-bit integer")?,
    ])
}

#[derive(Args, Debug)]
#[command(group(clap::ArgGroup::new("map_place").required(true).args(["here", "thing", "pos"])))]
pub struct NewMapArgs {
    /// One line, at most 200 characters.
    #[arg(long)]
    name: String,
    /// Place it at this actor's current position.
    #[arg(long)]
    here: bool,
    /// Place it at a sensed/remembered thing's last known position.
    #[arg(long)]
    thing: Option<u32>,
    /// Explicit x,z in millimetres (negative values are accepted).
    #[arg(long, value_parser = position_arg, allow_hyphen_values = true)]
    pos: Option<[i32; 2]>,
    #[arg(long)]
    kind: Option<String>,
    /// At most 8192 UTF-8 bytes.
    #[arg(long, default_value = "")]
    note: String,
}

#[derive(Args, Debug)]
pub struct MapChangeArgs {
    #[arg(long)]
    id: u32,
    #[arg(long)]
    expected_tick: u64,
    /// Exact updated.date, including any revision suffix.
    #[arg(long, default_value = "")]
    expected_date: String,
    #[arg(long)]
    name: Option<String>,
    #[arg(long, conflicts_with = "clear_kind")]
    kind: Option<String>,
    /// Remove the optional category.
    #[arg(long)]
    clear_kind: bool,
    #[arg(long, value_parser = position_arg, allow_hyphen_values = true)]
    pos: Option<[i32; 2]>,
    #[arg(long)]
    note: Option<String>,
}

#[derive(Subcommand, Debug)]
pub enum AgentMapCmd {
    /// Every landmark by id, with positions and optimistic update stamps.
    List {
        #[arg(long, required_unless_present = "agent_id")]
        unit: Option<u32>,
        #[arg(long, required_unless_present = "agent_id")]
        controller: Option<String>,
    },
    /// Mark here, a remembered thing, or a fixed position.
    Add {
        #[arg(long, required_unless_present = "agent_id")]
        unit: Option<u32>,
        #[arg(long, required_unless_present = "agent_id")]
        controller: Option<String>,
        #[command(flatten)]
        landmark: NewMapArgs,
    },
    /// Update only if the stored landmark still has the expected stamp.
    Update {
        #[arg(long, required_unless_present = "agent_id")]
        unit: Option<u32>,
        #[arg(long, required_unless_present = "agent_id")]
        controller: Option<String>,
        #[command(flatten)]
        change: MapChangeArgs,
    },
}

#[derive(Subcommand, Debug)]
pub enum MapCmd {
    /// List an actor's landmarks as the player.
    List {
        #[arg(long)]
        unit: u32,
    },
    /// Add a landmark as the player.
    Add {
        #[arg(long)]
        unit: u32,
        #[command(flatten)]
        landmark: NewMapArgs,
    },
    /// Update a landmark as the player, keeping its id and thing.
    Update {
        #[arg(long)]
        unit: u32,
        #[command(flatten)]
        change: MapChangeArgs,
    },
}

fn add_params(params: &mut Value, new: &NewMapArgs) {
    params["name"] = json!(new.name);
    params["note"] = json!(new.note);
    params["place"] = if new.here {
        json!("here")
    } else if let Some(thing) = new.thing {
        json!({"thing":thing})
    } else {
        json!({"pos":new.pos.expect("clap requires exactly one placement")})
    };
    if let Some(kind) = &new.kind {
        params["kind"] = json!(kind);
    }
}

fn update_params(params: &mut Value, change: &MapChangeArgs) {
    params["id"] = json!(change.id);
    params["expected"] = json!({"tick":change.expected_tick,"date":change.expected_date});
    if let Some(name) = &change.name {
        params["name"] = json!(name);
    }
    if change.clear_kind {
        params["kind"] = Value::Null;
    } else if let Some(kind) = &change.kind {
        params["kind"] = json!(kind);
    }
    if let Some(pos) = change.pos {
        params["pos"] = json!(pos);
    }
    if let Some(note) = &change.note {
        params["note"] = json!(note);
    }
}

pub(super) fn agent_map(cmd: &AgentMapCmd) -> (&'static str, Value) {
    match cmd {
        AgentMapCmd::List { unit, controller } => (
            "agent/map/list",
            json!({"unit":unit,"controller":controller}),
        ),
        AgentMapCmd::Add {
            unit,
            controller,
            landmark,
        } => {
            let mut params = json!({"unit":unit,"controller":controller});
            add_params(&mut params, landmark);
            ("agent/map/add", params)
        }
        AgentMapCmd::Update {
            unit,
            controller,
            change,
        } => {
            let mut params = json!({"unit":unit,"controller":controller});
            update_params(&mut params, change);
            ("agent/map/update", params)
        }
    }
}

pub fn cmd_map(cmd: &MapCmd, c: &ClientArgs, out: &mut Out) -> Result<u8, Failure> {
    let client = client(&c.url, c.transport_token()?)?;
    let (method, params) = match cmd {
        MapCmd::List { unit } => ("map/list", json!({"unit":unit})),
        MapCmd::Add { unit, landmark } => {
            let mut params = json!({"unit":unit});
            add_params(&mut params, landmark);
            ("map/add", params)
        }
        MapCmd::Update { unit, change } => {
            let mut params = json!({"unit":unit});
            update_params(&mut params, change);
            ("map/update", params)
        }
    };
    let result = rpc(&client, method, params)?;
    print(out, c.format, &result)?;
    Ok(exit_code(&result))
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[derive(Parser)]
    struct AgentArgs {
        #[command(flatten)]
        client: ClientArgs,
        #[command(subcommand)]
        cmd: AgentMapCmd,
    }

    #[test]
    fn placement_is_exclusive_and_negative_coordinates_are_preserved() {
        let parsed = AgentArgs::try_parse_from([
            "map",
            "add",
            "--agent-id",
            "pilot",
            "--token",
            "fixture",
            "--name",
            "North gate",
            "--pos",
            "-1200,2400",
            "--kind",
            "gate",
        ])
        .unwrap();
        assert_eq!(parsed.client.agent_id.as_deref(), Some("pilot"));
        let (method, params) = agent_map(&parsed.cmd);
        assert_eq!(method, "agent/map/add");
        assert_eq!(params["place"], json!({"pos":[-1200,2400]}));
        assert_eq!(params["kind"], "gate");
        assert!(
            AgentArgs::try_parse_from([
                "map",
                "add",
                "--agent-id",
                "pilot",
                "--token",
                "fixture",
                "--name",
                "A",
                "--here",
                "--thing",
                "1",
            ])
            .is_err()
        );
        assert!(
            AgentArgs::try_parse_from([
                "map",
                "add",
                "--agent-id",
                "pilot",
                "--token",
                "fixture",
                "--name",
                "A",
            ])
            .is_err()
        );
    }

    #[test]
    fn clearing_kind_is_distinct_from_an_omitted_change() {
        let parsed = AgentArgs::try_parse_from([
            "map",
            "update",
            "--agent-id",
            "pilot",
            "--token",
            "fixture",
            "--id",
            "3",
            "--expected-tick",
            "42",
            "--expected-date",
            "DATE#r7",
            "--clear-kind",
        ])
        .unwrap();
        let (_, params) = agent_map(&parsed.cmd);
        assert_eq!(params["expected"], json!({"tick":42,"date":"DATE#r7"}));
        assert!(params.get("kind").is_some_and(Value::is_null));
        assert!(params.get("name").is_none());
        assert!(params.get("pos").is_none());
    }
}
