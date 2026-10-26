//! A scripted, non-LLM reference controller (spec §2.8): reads an
//! observation, picks the first matching rule, acts, waits for the action to
//! finish. It knows nothing about any content: rules name a need (as shown
//! in the `needs` section, `{"<Need>": {"value": ..}}`) and a verb, and the
//! goal is a position reached with the `walk_to` verb.
//!
//! Used in process by tests ([`decide`] on `api::call` observations) and
//! over HTTP by `examples/greedy_controller` ([`run`]).

use std::time::Duration;

use serde_json::{Value, json};

use crate::http::{ClientError, RpcClient};

/// "When need `need` is below `below` (milli-units), do `verb`."
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NeedRule {
    pub need: String,
    pub below: i64,
    pub verb: String,
}

/// What the greedy controller wants.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Greedy {
    pub unit: u32,
    pub controller: String,
    /// Rules, by priority.
    pub rules: Vec<NeedRule>,
    /// Walk here (mm), then the goal is reached.
    pub goal: Option<(i32, i32)>,
    pub tolerance_mm: i64,
}

/// What to do next.
#[derive(Clone, Debug, PartialEq)]
pub enum Decision {
    /// Send these `agent/act` params.
    Act(Value),
    /// Something is still running (or nothing is available): wait.
    Wait,
    /// The goal is reached and no rule fires.
    Done,
}

fn offered<'a>(obs: &'a Value, verb: &str) -> Option<&'a Value> {
    obs["affordances"]
        .as_array()?
        .iter()
        .find(|a| a["verb"] == verb && a["ok"] == true)
}

/// Picks the next action for `obs` (an `agent/observe` JSON result).
pub fn decide(g: &Greedy, obs: &Value) -> Decision {
    let busy = obs["queue"]["current"].is_object()
        || obs["queue"]["pending"]
            .as_array()
            .is_some_and(|p| !p.is_empty());
    if busy {
        return Decision::Wait;
    }
    let stamp = |mut act: Value| {
        act["unit"] = json!(g.unit);
        act["controller"] = json!(g.controller);
        act["expect_tick"] = obs["tick"].clone();
        Decision::Act(act)
    };
    for rule in &g.rules {
        let value = obs["sections"]["needs"][&rule.need]["value"].as_i64();
        if value.is_some_and(|v| v < rule.below)
            && let Some(a) = offered(obs, &rule.verb)
        {
            return stamp(json!({"affordance": a["id"]}));
        }
    }
    if let Some((x, z)) = g.goal {
        let pos = &obs["sections"]["self"]["pos"];
        let (px, pz) = (pos[0].as_i64().unwrap_or(0), pos[1].as_i64().unwrap_or(0));
        let (dx, dz) = (i64::from(x) - px, i64::from(z) - pz);
        if dx * dx + dz * dz > g.tolerance_mm * g.tolerance_mm {
            if offered(obs, "walk_to").is_some() {
                return stamp(json!({"verb": "walk_to", "pos": [x, z]}));
            }
            return Decision::Wait;
        }
    }
    Decision::Done
}

/// Outcome of [`run`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunReport {
    pub acts: u32,
    pub done: bool,
    pub final_tick: u64,
}

/// Drives `g.unit` over HTTP until [`Decision::Done`] or `max_acts` acts:
/// observe → decide → act → wait for the action's `done`/`failed` event
/// (long-poll). Attach first (Direct).
pub fn run(client: &RpcClient, g: &Greedy, max_acts: u32) -> Result<RunReport, ClientError> {
    let mut acts = 0;
    let mut since = 0;
    loop {
        let obs = client.call(
            "agent/observe",
            json!({"unit": g.unit, "controller": g.controller}),
        )?;
        since = since.max(obs["seq"].as_u64().unwrap_or(0));
        match decide(g, &obs) {
            Decision::Done => {
                return Ok(RunReport {
                    acts,
                    done: true,
                    final_tick: obs["tick"].as_u64().unwrap_or(0),
                });
            }
            Decision::Act(params) => {
                if acts >= max_acts {
                    return Ok(RunReport {
                        acts,
                        done: false,
                        final_tick: obs["tick"].as_u64().unwrap_or(0),
                    });
                }
                let r = client.call("agent/act", params)?;
                acts += 1;
                if r["accepted"] != true {
                    std::thread::sleep(Duration::from_millis(50));
                    continue;
                }
                let handle = r["handle"].clone();
                // Wait for this action to end.
                for _ in 0..600 {
                    let ev = client.call(
                        "agent/events",
                        json!({"unit": g.unit, "since_seq": since, "kinds": ["ExternalAction"], "timeout_ms": 1000}),
                    )?;
                    since = ev["next_seq"].as_u64().unwrap_or(since);
                    let ended = ev["events"].as_array().is_some_and(|evs| {
                        evs.iter().any(|e| {
                            e["event"]["handle"] == handle
                                && matches!(
                                    e["event"]["status"].as_str(),
                                    Some("done" | "failed" | "cancelled")
                                )
                        })
                    });
                    if ended {
                        break;
                    }
                }
            }
            Decision::Wait => {
                let ev = client.call(
                    "agent/events",
                    json!({"unit": g.unit, "since_seq": since, "timeout_ms": 500}),
                )?;
                since = ev["next_seq"].as_u64().unwrap_or(since);
            }
        }
    }
}
