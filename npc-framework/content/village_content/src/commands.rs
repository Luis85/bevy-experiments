//! The village's content commands. On the wire they are ordinary
//! `SimCommand`s tagged by `"type"`:
//! `{"type":"SpawnVillager","pos":[1000,1000],"home":null}` and
//! `{"type":"SetFood","stockpile":3,"food":0}` (unchanged from before the
//! split, so old command files and replays still load).

use bevy_ecs::prelude::*;
use npc_framework::kernel::commands::{check_pos, entity, entity_with, invalid};
use npc_framework::prelude::*;
use serde::{Deserialize, Serialize};

use crate::scenario::villager_bundle;
use crate::villager::VillagerTree;
use crate::world::{Home, Stockpile};

/// A new villager at `pos` (mm), living in `home` (default: the
/// lowest-SimId home).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SpawnVillager {
    pub pos: (i32, i32),
    #[serde(default)]
    pub home: Option<u32>,
}

/// Sets a stockpile's food (`0..=capacity`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SetFood {
    pub stockpile: u32,
    pub food: u32,
}

/// Home a spawned villager lives in: `home`, or the lowest-SimId `Home`.
fn resolve_home(world: &World, home: Option<u32>) -> Result<SimId, CommandError> {
    match home {
        Some(h) => entity_with::<Home>(world, h, "home").map(|_| SimId(h)),
        None => {
            let index = world
                .get_resource::<SimIndex>()
                .ok_or_else(|| CommandError::NotReady {
                    missing: "SimIndex".to_owned(),
                })?;
            index
                .map
                .iter()
                .find(|(_, e)| world.get::<Home>(**e).is_some())
                .map(|(&id, _)| id)
                .ok_or_else(|| CommandError::Unavailable {
                    reason: "no home to spawn a villager into".to_owned(),
                })
        }
    }
}

impl ContentCommand for SpawnVillager {
    const NAME: &'static str = "SpawnVillager";

    fn validate(&self, world: &World) -> Result<(), CommandError> {
        check_pos("pos", self.pos)?;
        resolve_home(world, self.home)?;
        if world.get_resource::<VillagerTree>().is_none() {
            return Err(CommandError::NotReady {
                missing: "VillagerTree (VillagePlugin)".to_owned(),
            });
        }
        Ok(())
    }

    fn apply(self, world: &mut World) {
        let (Ok(home), Some(tree)) = (
            resolve_home(world, self.home),
            world.get_resource::<VillagerTree>().map(|t| t.0),
        ) else {
            return;
        };
        let bundle = villager_bundle(world, home, SimPos::new(self.pos.0, self.pos.1), tree);
        spawn_sim(world, bundle);
    }
}

impl ContentCommand for SetFood {
    const NAME: &'static str = "SetFood";

    fn validate(&self, world: &World) -> Result<(), CommandError> {
        let e = entity_with::<Stockpile>(world, self.stockpile, "stockpile")?;
        let capacity = world.get::<Stockpile>(e).map_or(0, |s| s.capacity);
        if self.food > capacity {
            return Err(invalid(
                "food",
                format!("{} exceeds capacity {capacity}", self.food),
            ));
        }
        Ok(())
    }

    fn apply(self, world: &mut World) {
        if let Ok(e) = entity(world, self.stockpile)
            && let Some(mut pile) = world.get_mut::<Stockpile>(e)
        {
            pile.food = self.food;
        }
    }

    fn unit(&self) -> Option<SimId> {
        Some(SimId(self.stockpile))
    }
}
