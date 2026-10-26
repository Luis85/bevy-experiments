//! Rules × agent-io (features `rules` + `agent-io`): the character sheet in
//! observations and spending points as an agent action.
//!
//! [`RulesAgentPlugin`] registers:
//!
//! - observation section [`SHEET_SECTION`] (`"sheet"`), from the unit's
//!   `SheetView`:
//!   `{"rule_set":..,"attributes":{"DX":12,..},"secondary":{"HP":10,..,"Speed":"5.75","Move":5},
//!     "traits":[{"name","level"}],"skills":[{"name","level","relative","points"}],
//!     "points":{"unspent":10,"total":..},"used_defaults":[{"name","level","default"}]}`
//!   (skills are the bought ones, by name, with their effective levels).
//!   With the `progression` feature, a skill the unit has used (rolled)
//!   without buying it is listed too, at its default: in `used_defaults`
//!   (name order) and on the Skills line (`"Carpentry 7 (default DX-5)"`);
//! - affordance provider `npc_rules.spend_points` and handler
//!   [`SPEND_POINTS`] (`"spend_points"`, args `{"purchase": Purchase}`, e.g.
//!   `{"purchase":{"type":"Skill","skill":"Climbing","points":2}}`; works in
//!   `Direct` and `Advisor` mode, `why_not` without unspent points);
//! - the recorded content command `npc_rules::SpendPoints {unit, purchase}`
//!   (the handler submits it), so spending is validated at apply time,
//!   journaled and replayable.
//!
//! BRP `npc/sheet {unit}` is `npc_tools`' (features `remote` + `rules`).

#![deny(
    clippy::float_arithmetic,
    clippy::float_cmp,
    clippy::iter_over_hash_type,
    clippy::disallowed_types,
    clippy::disallowed_methods
)]

use bevy_app::{App, Plugin};
use bevy_ecs::prelude::*;
use npc_agent_io::{
    ActionPlan, ActionRequest, Affordance, AgentCtx, AgentIoAppExt, ControlMode, Rejection, Section,
};
use npc_kernel::SimCommand;
use npc_kernel::registry::KernelAppExt;
use npc_rules::{Purchase, SpendError, SpendPoints, Stat, sheet_view};
use serde_json::{Map, Value, json};

/// Name of the sheet section.
pub const SHEET_SECTION: &str = "sheet";
/// The spending verb.
pub const SPEND_POINTS: &str = "spend_points";

/// The `sheet` section of `ctx.unit` (`None` without a character sheet).
pub fn sheet_section(world: &World, ctx: &AgentCtx) -> Option<Section> {
    let view = sheet_view(world, ctx.unit)?;
    let mut attributes = Map::new();
    let mut secondary = Map::new();
    let mut attr_text = Vec::new();
    let mut sec_text = Vec::new();
    for s in &view.stats {
        if Stat::ATTRIBUTES.contains(&s.stat) {
            attributes.insert(s.name.clone(), json!(s.value));
            attr_text.push(format!("{} {}", s.name, s.value));
        } else if s.stat == Stat::Speed {
            secondary.insert(s.name.clone(), json!(view.basic_speed));
            sec_text.push(format!("{} {}", s.name, view.basic_speed));
        } else {
            secondary.insert(s.name.clone(), json!(s.value));
            sec_text.push(format!("{} {}", s.name, s.value));
        }
    }
    let traits: Vec<Value> = view
        .traits
        .iter()
        .map(|t| json!({"name": t.name, "level": t.level}))
        .collect();
    let skills: Vec<Value> = view
        .skills
        .iter()
        .map(|s| {
            json!({
                "name": s.name, "level": s.level, "relative": s.relative_label, "points": s.points,
            })
        })
        .collect();
    let skill_text: Vec<String> = view
        .skills
        .iter()
        .filter_map(|s| {
            let level = s.level?;
            Some(match &s.relative_label {
                Some(r) => format!("{} {level} ({r})", s.name),
                None => format!("{} {level}", s.name),
            })
        })
        .collect();
    let defaults = used_defaults(world, ctx, &view);
    let mut skill_text = skill_text;
    skill_text.extend(
        defaults
            .iter()
            .map(|d| format!("{} {} (default {})", d.0, d.1, d.2)),
    );
    let used: Vec<Value> = defaults
        .iter()
        .map(|d| json!({"name": d.0, "level": d.1, "default": d.2}))
        .collect();
    let mut section = Section::new(json!({
        "rule_set": view.rule_set,
        "attributes": attributes,
        "secondary": secondary,
        "traits": traits,
        "skills": skills,
        "points": {"unspent": view.points.unspent, "total": view.points.total},
        "used_defaults": used,
    }))
    .line(format!(
        "Sheet: {}; {}.",
        attr_text.join(", "),
        sec_text.join(", ")
    ));
    if !view.traits.is_empty() {
        let t: Vec<String> = view
            .traits
            .iter()
            .map(|t| format!("{} {}", t.name, t.level))
            .collect();
        section = section.line(format!("Traits: {}.", t.join(", ")));
    }
    if !skill_text.is_empty() {
        section = section.line(format!("Skills: {}.", skill_text.join(", ")));
    }
    Some(section.line(format!(
        "Unspent character points: {}.",
        view.points.unspent
    )))
}

/// The skills `ctx.unit` used (rolled; `npc_progression::Learning`) without
/// buying them, at their defaults: `(name, level, "DX-5")`, name order.
#[cfg(feature = "progression")]
fn used_defaults(
    world: &World,
    ctx: &AgentCtx,
    view: &npc_rules::SheetView,
) -> Vec<(String, i32, String)> {
    let Some(learning) = world.get::<npc_progression::Learning>(ctx.entity) else {
        return Vec::new();
    };
    learning
        .skills
        .iter()
        .filter(|(name, p)| p.uses > 0 && !view.skills.iter().any(|s| &s.name == *name))
        .filter_map(|(name, _)| {
            let level = npc_rules::skill_level(world, ctx.unit, name)?;
            match level.source {
                npc_rules::LevelSource::Default { from, modifier } => {
                    Some((name.clone(), level.level, format!("{from}{modifier:+}")))
                }
                npc_rules::LevelSource::Points => None,
            }
        })
        .collect()
}

/// Without progression nothing records which skills were used.
#[cfg(not(feature = "progression"))]
fn used_defaults(
    _world: &World,
    _ctx: &AgentCtx,
    _view: &npc_rules::SheetView,
) -> Vec<(String, i32, String)> {
    Vec::new()
}

const PURCHASE_SCHEMA: &str = "{\"type\":\"Skill\",\"skill\":NAME,\"points\":N} | \
     {\"type\":\"Stat\",\"stat\":\"St|Dx|Iq|Ht|Hp|Will|Per|Fp|Speed|Move\",\"levels\":N} | \
     {\"type\":\"Trait\",\"name\":NAME,\"levels\":N}";

/// Affordance provider `npc_rules.spend_points`: offered to units with a
/// character sheet.
pub fn spend_points_affordances(world: &World, ctx: &AgentCtx) -> Vec<Affordance> {
    let Some(sheet) = world.get::<npc_rules::CharacterSheet>(ctx.entity) else {
        return Vec::new();
    };
    let a = Affordance::new(
        SPEND_POINTS,
        format!("Spend banked character points ({} unspent)", sheet.unspent),
    )
    .arg("purchase", PURCHASE_SCHEMA)
    .requires("unspent_points");
    let a = match ctx.mode() {
        ControlMode::Observer => a.why_not("observers cannot act"),
        _ if sheet.unspent <= 0 => a.why_not("no unspent character points"),
        _ => a,
    };
    vec![a]
}

fn describe(p: &Purchase) -> String {
    match p {
        Purchase::Skill { skill, points } => format!("{points} points in {skill}"),
        Purchase::Stat { stat, levels } => format!("+{levels} {}", stat.name()),
        Purchase::Trait { name, levels } => format!("{levels} level(s) of {name}"),
    }
}

fn rejection(e: &SpendError) -> Rejection {
    let reason = match e {
        SpendError::NotEnoughPoints { .. } => "not_enough_points",
        SpendError::NoSheet { .. } => "no_sheet",
        _ => "invalid_args",
    };
    Rejection::new(
        reason,
        serde_json::to_string(e).unwrap_or_else(|_| format!("{e:?}")),
    )
}

/// Handler of `spend_points {purchase}`: validates the purchase now and
/// submits [`SpendPoints`] (validated again when it applies).
pub fn spend_points_handler(
    world: &World,
    ctx: &AgentCtx,
    req: &ActionRequest,
) -> Result<ActionPlan, Rejection> {
    let raw = req.args.get("purchase").cloned().ok_or_else(|| {
        Rejection::new(
            "invalid_args",
            format!("args.purchase is required: {PURCHASE_SCHEMA}"),
        )
    })?;
    let purchase: Purchase = serde_json::from_value(raw).map_err(|e| {
        Rejection::new(
            "invalid_args",
            format!("bad purchase ({e}); expected {PURCHASE_SCHEMA}"),
        )
    })?;
    let cmd = SpendPoints {
        unit: ctx.unit.0,
        purchase,
    };
    let cost = cmd.check(world).map_err(|e| rejection(&e))?;
    Ok(ActionPlan::Commands {
        label: format!("Spend {cost} points: {}", describe(&cmd.purchase)),
        commands: vec![SimCommand::content(&cmd)],
    })
}

/// Rules × agent-io glue (see the module docs). Add it with `AgentIoPlugin`
/// and `RulesPlugin`. It registers the `SpendPoints` command; do not
/// register that command yourself as well.
pub struct RulesAgentPlugin;

impl Plugin for RulesAgentPlugin {
    fn build(&self, app: &mut App) {
        app.register_command::<SpendPoints>()
            .register_observation(SHEET_SECTION, sheet_section)
            .register_affordances("npc_rules.spend_points", spend_points_affordances)
            .register_action_handler(SPEND_POINTS, spend_points_handler);
    }
}
