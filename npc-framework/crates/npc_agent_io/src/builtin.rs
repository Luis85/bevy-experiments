//! Generic affordances every unit gets: `wait`.

use bevy_ecs::prelude::*;
use serde_json::json;

use crate::control::{ControlMode, MAX_WAIT_TICKS, Step};
use crate::registry::{ActionPlan, ActionRequest, Affordance, AgentCtx, Rejection};

/// Default `wait` length in ticks.
pub const DEFAULT_WAIT_TICKS: u32 = 100;

/// Affordance provider `npc_agent_io.wait`.
pub fn wait_affordances(_world: &World, ctx: &AgentCtx) -> Vec<Affordance> {
    let a = Affordance::new("wait", "Wait in place")
        .arg("ticks", "ticks to wait, 1..=1000000 (default 100)")
        .est_ticks(DEFAULT_WAIT_TICKS);
    let a = match ctx.mode() {
        ControlMode::Direct => a,
        ControlMode::Advisor => a.why_not("waiting needs Direct control"),
        ControlMode::Observer => a.why_not("observers cannot act"),
    };
    vec![a]
}

/// Handler of `wait {ticks}`.
pub fn wait_handler(
    _world: &World,
    _ctx: &AgentCtx,
    req: &ActionRequest,
) -> Result<ActionPlan, Rejection> {
    let ticks = req
        .opt_int("ticks")?
        .unwrap_or(i64::from(DEFAULT_WAIT_TICKS));
    let ticks = u32::try_from(ticks)
        .ok()
        .filter(|t| (1..=MAX_WAIT_TICKS).contains(t))
        .ok_or_else(|| {
            Rejection::new(
                "invalid_args",
                format!(
                    "ticks must be 1..={MAX_WAIT_TICKS}, got {}",
                    json!(req.args["ticks"])
                ),
            )
        })?;
    Ok(ActionPlan::Queue {
        label: format!("Wait {ticks} ticks"),
        steps: vec![Step::Wait { ticks }],
        est_ticks: Some(ticks),
    })
}
