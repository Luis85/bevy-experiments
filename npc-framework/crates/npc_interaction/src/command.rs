//! The `Interact` content command: `{"type":"Interact","unit":5,"verb":"open","target":12}`
//! (optional `"item"`). Validated against the same checks as the leaf, so a
//! refused command is rejected with the reason (`"door is locked"`); applied
//! through the same [`begin`](crate::run::begin).

use bevy_ecs::prelude::*;
use npc_kernel::commands::{CommandError, entity, entity_with};
use npc_kernel::{ContentCommand, SimId};
use serde::{Deserialize, Serialize};

use crate::items::{Interactor, with_room_hint};
use crate::run::{Source, begin, busy_with, can_interact};

/// `unit` does `verb` to `target` (with `item`).
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Interact {
    pub unit: u32,
    pub verb: String,
    pub target: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item: Option<u32>,
}

impl Interact {
    pub fn new(unit: SimId, verb: &str, target: SimId) -> Self {
        Self {
            unit: unit.0,
            verb: verb.to_owned(),
            target: target.0,
            item: None,
        }
    }

    pub fn with_item(mut self, item: SimId) -> Self {
        self.item = Some(item.0);
        self
    }
}

fn unavailable(reason: String) -> CommandError {
    CommandError::Unavailable { reason }
}

impl ContentCommand for Interact {
    const NAME: &'static str = "Interact";

    fn validate(&self, world: &World) -> Result<(), CommandError> {
        let actor = entity_with::<Interactor>(world, self.unit, "interactor")?;
        entity(world, self.target)?;
        if let Some(item) = self.item {
            entity(world, item)?;
        }
        if let Some(b) = busy_with(world, actor) {
            return Err(unavailable(format!("unit {} is busy with {b}", self.unit)));
        }
        can_interact(
            world,
            actor,
            &self.verb,
            SimId(self.target),
            self.item.map(SimId),
        )
        .map(|_| ())
        .map_err(|why| unavailable(with_room_hint(world, actor, why)))
    }

    fn apply(self, world: &mut World) {
        if let Ok(actor) = entity(world, self.unit) {
            let _ = begin(
                world,
                actor,
                &self.verb,
                SimId(self.target),
                self.item.map(SimId),
                Source::Command,
            );
        }
    }

    fn unit(&self) -> Option<SimId> {
        Some(SimId(self.unit))
    }
}
