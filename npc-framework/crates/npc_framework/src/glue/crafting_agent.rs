//! Crafting × agent-io (features `crafting` + `agent-io`): crafting as an
//! agent action, and the `crafting` section.
//!
//! [`CraftingAgentPlugin`] registers:
//!
//! - affordance provider `npc_crafting`: verb `craft:<recipe>` for every
//!   registered recipe (name order), for units that can craft
//!   (`Interactor` + `Inventory`). A recipe that needs a workstation is
//!   offered once per workstation the unit knows (with a perception
//!   provider: perceives or remembers, at any distance; without: within
//!   `InteractionConfig::affordance_radius_mm`) that hosts it (id
//!   `craft:<recipe>@<workstation>`, target the workstation), or once without
//!   a target when none is near (`why_not: "needs a workstation: forge"`);
//!   a recipe without one is offered once (id `craft:<recipe>`). `why_not`
//!   is `npc_crafting::can_craft`'s `CraftError` (`"needs 2 flour (has 0)"`;
//!   with several inputs or tools every one with what the unit has, the
//!   missing first: `"needs 1 plank (has 0), 1 ingot (has 2), tool hammer
//!   (has 1)"`; `"too far: .."`, `"does not know Weaving"`, `"#12 is in
//!   use"`) or `"busy with .."`. The label says what the craft consumes and
//!   uses (`"Craft forge_axe at workstation #14: consumes 1 ingot, 1 plank;
//!   uses hammer"`) and, for a unit with a character sheet and a recipe with
//!   a skill, its odds like a tool use (`check`, and `" [Carpentry 7
//!   (default DX-5) vs 3d6: ~16%, low odds]"`: the effective skill plus the
//!   recipe's modifier and the planned tools' quality, as the craft rolls
//!   it);
//! - handlers `craft:<recipe>` (one per recipe, registered in
//!   `Plugin::finish`) and `craft {"recipe": NAME}` (target: the
//!   workstation): `Direct` queues the
//!   `Craft` leaf (`SetBlackboard target_slot = workstation`, `Action Craft
//!   <recipe>`); `Advisor` submits the recorded `Craft` command. Without a
//!   recipe and with a workstation, the first posted order on it the unit
//!   can do (`next_order_for`);
//! - observation section [`CRAFTING_SECTION`] (`"crafting"`): the running
//!   craft and the workstations near the unit (kinds, posted orders), with
//!   text lines.

#![deny(
    clippy::float_arithmetic,
    clippy::float_cmp,
    clippy::iter_over_hash_type,
    clippy::disallowed_types,
    clippy::disallowed_methods
)]

use bevy_app::{App, Plugin};
use bevy_ecs::prelude::*;
use npc_agent_io::observe::known_targets;
use npc_agent_io::{
    ActionPlan, ActionRequest, Affordance, AgentCtx, AgentIoAppExt, ControlMode, Rejection,
    Section, Step,
};
use npc_ai::{BbValue, ParamValue};
use npc_crafting::{
    CRAFT_ACTION, Craft, CraftError, Crafting, CraftingOrders, RecipeDef, RecipeRegistry,
    Workstation, busy_with, can_craft, next_order_for, tool_name,
};
use npc_interaction::Item;
use npc_interaction::items::entity_of;
use npc_interaction::{InteractionConfig, Interactor, Inventory};
use npc_kernel::spatial::SpatialGrid;
use npc_kernel::{SimCommand, SimId, SimPos};
use serde_json::json;

/// Name of the section.
pub const CRAFTING_SECTION: &str = "crafting";
/// The crafting verb.
pub const CRAFT: &str = "craft";

fn radius(world: &World) -> i32 {
    world
        .get_resource::<InteractionConfig>()
        .copied()
        .unwrap_or_default()
        .affordance_radius_mm
}

fn is_workstation(world: &World, id: SimId) -> bool {
    entity_of(world, id).is_some_and(|e| world.get::<Workstation>(e).is_some())
}

/// The workstations `ctx.unit` knows (with perception: perceives or
/// remembers), else those within the affordance radius, ascending.
pub fn known_workstations(world: &World, ctx: &AgentCtx) -> Vec<SimId> {
    match known_targets(world, ctx) {
        Some(known) => known
            .into_iter()
            .filter(|&id| is_workstation(world, id))
            .collect(),
        None => nearby_workstations(world, ctx.entity),
    }
}

/// Workstations within the affordance radius of `actor`, ascending.
pub fn nearby_workstations(world: &World, actor: Entity) -> Vec<SimId> {
    let (Some(grid), Some(&pos)) = (
        world.get_resource::<SpatialGrid>(),
        world.get::<SimPos>(actor),
    ) else {
        return Vec::new();
    };
    let mut out: Vec<SimId> = grid
        .in_radius(pos, radius(world))
        .into_iter()
        .filter(|&id| is_workstation(world, id))
        .collect();
    out.sort_unstable();
    out
}

/// `": consumes 1 ingot, 1 plank; uses hammer"` of a recipe (empty without
/// inputs or tools).
pub fn uses_note(def: &RecipeDef) -> String {
    let mut parts = Vec::new();
    if !def.inputs.is_empty() {
        let i: Vec<String> = def
            .inputs
            .iter()
            .map(|i| format!("{} {}", i.count, i.key))
            .collect();
        parts.push(format!("consumes {}", i.join(", ")));
    }
    if !def.tools.is_empty() {
        let t: Vec<&str> = def.tools.iter().map(|t| tool_name(t)).collect();
        parts.push(format!("uses {}", t.join(", ")));
    }
    if parts.is_empty() {
        String::new()
    } else {
        format!(": {}", parts.join("; "))
    }
}

/// The roll a craft of `def` by `actor` would make, as the affordance's
/// `check` JSON (see the module docs); `None` without a skill or a sheet.
/// The tools are the planned ones when the craft can start (their quality
/// adds to the roll), else none.
fn craft_check(
    world: &World,
    actor: Entity,
    def: &RecipeDef,
    tools: &[SimId],
) -> Option<serde_json::Value> {
    let skill = def.skill.as_deref()?;
    let me = *world.get::<SimId>(actor)?;
    let tool_bonus: i32 = tools
        .iter()
        .filter_map(|&t| entity_of(world, t))
        .filter_map(|e| world.get::<Item>(e))
        .map(|i| i.quality)
        .sum();
    crate::glue::interaction_agent::odds::skill_check(
        world,
        actor,
        me,
        skill,
        def.modifier.saturating_add(tool_bonus),
    )
}

fn recipe_affordance(
    world: &World,
    actor: Entity,
    def: &RecipeDef,
    workstation: Option<SimId>,
    gate: Option<&str>,
) -> Affordance {
    let verb = format!("{CRAFT}:{}", def.name);
    let plan = can_craft(world, actor, &def.name, workstation);
    let tools = plan.as_ref().map(|p| p.tools.clone()).unwrap_or_default();
    let check = craft_check(world, actor, def, &tools);
    let mut label = match workstation {
        Some(ws) => format!("Craft {} at workstation #{}", def.name, ws.0),
        None => format!("Craft {}", def.name),
    };
    label.push_str(&uses_note(def));
    if let Some(c) = &check {
        label.push_str(&crate::glue::interaction_agent::odds::label_suffix(c));
    }
    let mut a = Affordance::new(&verb, label).est_ticks(def.duration_ticks);
    a.check = check;
    if let Some(ws) = workstation {
        a = a.target(ws);
    }
    for i in &def.inputs {
        a = a.requires(&format!("{} x{}", i.key, i.count));
    }
    for t in &def.tools {
        a = a.requires(&format!("tool {t}"));
    }
    if let Some(why) = gate {
        return a.why_not(why);
    }
    match plan {
        Ok(_) => a,
        Err(e) => a.why_not(e.to_string()),
    }
}

/// Affordance provider `npc_crafting` (see the module docs).
pub fn crafting_affordances(world: &World, ctx: &AgentCtx) -> Vec<Affordance> {
    let actor = ctx.entity;
    if world.get::<Interactor>(actor).is_none() || world.get::<Inventory>(actor).is_none() {
        return Vec::new();
    }
    let Some(recipes) = world.get_resource::<RecipeRegistry>() else {
        return Vec::new();
    };
    let gate = match ctx.mode() {
        ControlMode::Observer => Some("observers cannot act".to_owned()),
        _ => busy_with(world, actor).map(|b| format!("busy with {b}")),
    };
    let stations = known_workstations(world, ctx);
    let mut out = Vec::new();
    for def in recipes.recipes.values() {
        if def.stations.is_empty() {
            out.push(recipe_affordance(world, actor, def, None, gate.as_deref()));
            continue;
        }
        let hosts: Vec<SimId> = stations
            .iter()
            .copied()
            .filter(|&ws| {
                entity_of(world, ws)
                    .and_then(|e| world.get::<Workstation>(e))
                    .is_some_and(|w| def.hosted_by(&w.kinds))
            })
            .collect();
        if hosts.is_empty() {
            let why = CraftError::NeedsWorkstation {
                needs: def.stations.clone(),
            }
            .to_string();
            out.push(recipe_affordance(world, actor, def, None, Some(&why)));
        }
        for ws in hosts {
            out.push(recipe_affordance(
                world,
                actor,
                def,
                Some(ws),
                gate.as_deref(),
            ));
        }
    }
    out
}

/// Handler of `craft` (see the module docs).
pub fn craft_handler(
    world: &World,
    ctx: &AgentCtx,
    req: &ActionRequest,
) -> Result<ActionPlan, Rejection> {
    let workstation = req.target;
    let named = req
        .verb
        .strip_prefix(CRAFT)
        .and_then(|r| r.strip_prefix(':'))
        .filter(|r| !r.is_empty());
    let recipe = match named.or(req.opt_str("recipe")?) {
        Some(r) => r.to_owned(),
        None => match workstation {
            Some(ws) => next_order_for(world, ctx.entity, ws)
                .map(|(_, r)| r)
                .map_err(|e| Rejection::new("unavailable", e.to_string()))?,
            None => {
                return Err(Rejection::new(
                    "invalid_args",
                    "craft needs args.recipe (or a workstation with posted orders)",
                ));
            }
        },
    };
    if let Some(b) = busy_with(world, ctx.entity) {
        return Err(Rejection::new("unavailable", format!("busy with {b}")));
    }
    let plan = can_craft(world, ctx.entity, &recipe, workstation)
        .map_err(|e| Rejection::new("unavailable", e.to_string()))?;
    let label = match workstation {
        Some(ws) => format!("Craft {recipe} at workstation #{}", ws.0),
        None => format!("Craft {recipe}"),
    };
    let est = Some(plan.recipe.duration_ticks);
    match ctx.mode() {
        ControlMode::Direct => {
            let slot = world
                .get_resource::<InteractionConfig>()
                .copied()
                .unwrap_or_default()
                .target_slot;
            let mut steps = Vec::new();
            if let Some(ws) = workstation {
                steps.push(Step::SetBlackboard {
                    key: slot,
                    value: BbValue::U32(ws.0),
                });
            }
            steps.push(Step::Action {
                action: CRAFT_ACTION.to_owned(),
                param: ParamValue::Name(recipe),
            });
            Ok(ActionPlan::Queue {
                label,
                steps,
                est_ticks: est,
            })
        }
        _ => {
            let mut cmd = Craft::new(ctx.unit, &recipe);
            if let Some(ws) = workstation {
                cmd = cmd.at(ws);
            }
            Ok(ActionPlan::Commands {
                label,
                commands: vec![SimCommand::content(&cmd)],
            })
        }
    }
}

/// Section `crafting` (see the module docs).
pub fn crafting_section(world: &World, ctx: &AgentCtx) -> Option<Section> {
    let actor = ctx.entity;
    let running = world.get::<Crafting>(actor);
    let crafter =
        world.get::<Inventory>(actor).is_some() && world.get::<Interactor>(actor).is_some();
    if running.is_none() && !crafter {
        return None;
    }
    let stations: Vec<serde_json::Value> = known_workstations(world, ctx)
        .into_iter()
        .filter_map(|id| {
            let e = entity_of(world, id)?;
            let w = world.get::<Workstation>(e)?;
            Some(json!({
                "id": id,
                "kinds": w.kinds,
                "orders": world.get::<CraftingOrders>(e).map(|o| &o.orders),
            }))
        })
        .collect();
    let n = stations.len();
    let mut section = Section::new(json!({
        "crafting": running,
        "workstations": stations,
    }));
    section = match running {
        Some(c) => section.line(format!(
            "You are crafting {} (done at tick {}).",
            c.recipe, c.ends
        )),
        None => section.line("You are not crafting."),
    };
    Some(section.line(format!("{n} workstation(s) known.")))
}

/// Crafting × agent-io glue (see the module docs). Add it with
/// `CraftingPlugin` and `AgentIoPlugin`.
pub struct CraftingAgentPlugin;

impl Plugin for CraftingAgentPlugin {
    fn build(&self, app: &mut App) {
        app.register_observation(CRAFTING_SECTION, crafting_section)
            .register_affordances("npc_crafting", crafting_affordances)
            .register_busy("npc_crafting", |world, e| {
                world
                    .get::<Crafting>(e)
                    .map(|c| format!("crafting {}", c.recipe))
            })
            .register_action_handler(CRAFT, craft_handler);
    }

    /// One handler per recipe verb `craft:<recipe>` (the recipes are known
    /// once every plugin is built).
    fn finish(&self, app: &mut App) {
        let names: Vec<String> = app
            .world()
            .get_resource::<RecipeRegistry>()
            .map(|r| r.recipes.keys().cloned().collect())
            .unwrap_or_default();
        let mut registry = app
            .world_mut()
            .get_resource_or_init::<npc_agent_io::AgentRegistry>();
        for name in names {
            // Bounded: one short string per registered recipe and app.
            let verb: &'static str = Box::leak(format!("{CRAFT}:{name}").into_boxed_str());
            registry.handlers.entry(verb).or_insert(craft_handler);
        }
    }
}
