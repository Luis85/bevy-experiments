//! The content commands: `Craft`
//! (`{"type":"Craft","unit":5,"recipe":"bread","workstation":12}`,
//! `workstation` optional) and `PostCraftOrder`
//! (`{"type":"PostCraftOrder","workstation":12,"recipe":"bread","count":3}`).
//! Validated against the same checks as the leaf, so a refused command is
//! rejected with the reason (`"needs 2 flour (has 0)"`).

use bevy_ecs::prelude::*;
use npc_kernel::commands::{CommandError, entity, entity_with};
use npc_kernel::{ContentCommand, SimId};
use serde::{Deserialize, Serialize};

use crate::run::{CraftSource, begin_craft, busy_with, can_craft};
use crate::station::{Workstation, check_order, post_order};

/// `unit` crafts `recipe` (at `workstation`).
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Craft {
    pub unit: u32,
    pub recipe: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workstation: Option<u32>,
}

impl Craft {
    pub fn new(unit: SimId, recipe: &str) -> Self {
        Self {
            unit: unit.0,
            recipe: recipe.to_owned(),
            workstation: None,
        }
    }

    pub fn at(mut self, workstation: SimId) -> Self {
        self.workstation = Some(workstation.0);
        self
    }
}

fn unavailable(reason: String) -> CommandError {
    CommandError::Unavailable { reason }
}

impl ContentCommand for Craft {
    const NAME: &'static str = "Craft";

    fn validate(&self, world: &World) -> Result<(), CommandError> {
        let actor = entity(world, self.unit)?;
        if let Some(b) = busy_with(world, actor) {
            return Err(unavailable(format!("unit {} is busy with {b}", self.unit)));
        }
        can_craft(world, actor, &self.recipe, self.workstation.map(SimId))
            .map(|_| ())
            .map_err(|e| unavailable(e.to_string()))
    }

    fn apply(self, world: &mut World) {
        if let Ok(actor) = entity(world, self.unit) {
            let _ = begin_craft(
                world,
                actor,
                &self.recipe,
                self.workstation.map(SimId),
                CraftSource::Command,
                None,
            );
        }
    }

    fn unit(&self) -> Option<SimId> {
        Some(SimId(self.unit))
    }
}

/// Posts an order of `count` crafts of `recipe` on `workstation`.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PostCraftOrder {
    pub workstation: u32,
    pub recipe: String,
    pub count: u32,
}

impl ContentCommand for PostCraftOrder {
    const NAME: &'static str = "PostCraftOrder";

    fn validate(&self, world: &World) -> Result<(), CommandError> {
        entity_with::<Workstation>(world, self.workstation, "workstation")?;
        check_order(world, SimId(self.workstation), &self.recipe, self.count)
            .map_err(|e| unavailable(e.to_string()))
    }

    fn apply(self, world: &mut World) {
        let _ = post_order(world, SimId(self.workstation), &self.recipe, self.count);
    }

    fn unit(&self) -> Option<SimId> {
        Some(SimId(self.workstation))
    }
}
