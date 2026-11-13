//! The testbed side of external agent control: two observation sections,
//! `testbed` (who the unit is: template, behaviour, owner, home, the vehicle
//! it rides) and `walls` (the wall runs and openings around it,
//! [`crate::walls`]). Everything an agent can *do* comes from the capability glue
//! (interaction verbs, `spend_points`, the `perception` and `sheet`
//! sections; orders, mounting and crafting affordances arrive with the
//! wave F4 glue), so this module registers no affordances of its own.
//!
//! The scenario loader is [`crate::scenario::register_scenario_loader`].

use bevy_app::{App, Plugin};
use npc_framework::agent_io::{AgentCtx, AgentIoAppExt, Section};
use npc_framework::prelude::*;
use serde_json::json;

use crate::walls::{WALLS_SECTION, walls_section};
use crate::world::TestbedUnit;

/// Registers the `testbed` and `walls` observation sections (add next to
/// `AgentIoPlugin`; the scenario loader adds it).
pub struct TestbedAgentPlugin;

impl Plugin for TestbedAgentPlugin {
    fn build(&self, app: &mut App) {
        app.register_observation("testbed", testbed_section)
            .register_observation(WALLS_SECTION, walls_section);
    }
}

fn testbed_section(world: &bevy_ecs::world::World, ctx: &AgentCtx) -> Option<Section> {
    let unit = world.get::<TestbedUnit>(ctx.entity)?;
    let owner = world.get::<Owner>(ctx.entity).map(|o| o.0.0);
    let riding = world.get::<MountedIn>(ctx.entity).map(|m| m.vehicle);
    let value = json!({
        "template": unit.template,
        "behaviour": unit.behaviour,
        "owner": owner,
        "home": [unit.home.x, unit.home.z],
        "speed_mm_per_tick": unit.speed_mm_per_tick,
        "riding": riding,
    });
    let mut section = Section::new(value).line(format!(
        "You are a {} of player {}; you walk {} mm per tick.",
        unit.template,
        owner.unwrap_or(0),
        unit.speed_mm_per_tick
    ));
    if let Some(v) = riding {
        section = section.line(format!("You ride vehicle #{}.", v.0));
    }
    Some(section)
}
