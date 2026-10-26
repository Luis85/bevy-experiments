//! The scripted reference controller over HTTP.
//!
//! ```sh
//! npcctl serve --scenario content/village_content/scenarios/agent_village.json &
//! cargo run -p npc_agent_io --example greedy_controller -- \
//!     --url http://127.0.0.1:15702 --unit 5 --goal 4000,4000 --eat-below 40000
//! ```
//!
//! Attaches to `--unit` (Direct), then: eat when `Hunger` is below
//! `--eat-below`, else walk to `--goal`; prints a JSON report and exits 0
//! when the goal is reached.

use std::process::ExitCode;

use npc_agent_io::greedy::{Greedy, NeedRule, run};
use npc_agent_io::http::RpcClient;
use serde_json::json;

fn arg(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1).cloned())
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let url = arg(&args, "--url").unwrap_or_else(|| "http://127.0.0.1:15702".into());
    let unit: u32 = arg(&args, "--unit")
        .and_then(|u| u.parse().ok())
        .unwrap_or(5);
    let controller = arg(&args, "--controller").unwrap_or_else(|| "greedy".into());
    let goal = arg(&args, "--goal").and_then(|g| {
        let (x, z) = g.split_once(',')?;
        Some((x.trim().parse().ok()?, z.trim().parse().ok()?))
    });
    let eat_below: i64 = arg(&args, "--eat-below")
        .and_then(|v| v.parse().ok())
        .unwrap_or(30_000);
    let client = match RpcClient::new(&url) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("greedy_controller: {e}");
            return ExitCode::from(2);
        }
    };
    if let Err(e) = client.call(
        "agent/attach",
        json!({"unit": unit, "controller": controller, "mode": "Direct", "force": true}),
    ) {
        eprintln!("greedy_controller: attach failed: {e}");
        return ExitCode::from(2);
    }
    let g = Greedy {
        unit,
        controller,
        rules: vec![NeedRule {
            need: "Hunger".into(),
            below: eat_below,
            verb: "eat".into(),
        }],
        goal,
        tolerance_mm: 100,
    };
    match run(&client, &g, 50) {
        Ok(r) => {
            println!(
                "{}",
                json!({"acts": r.acts, "done": r.done, "final_tick": r.final_tick})
            );
            if r.done {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(1)
            }
        }
        Err(e) => {
            eprintln!("greedy_controller: {e}");
            ExitCode::from(1)
        }
    }
}
