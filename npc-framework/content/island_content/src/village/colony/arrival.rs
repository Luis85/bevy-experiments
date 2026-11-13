//! The one-time visitor arrival command and its shared agent action adapter.
use super::super::{Role, VillageState, Villager, Vitality};
use super::{AwaitingArrival, home_of};
use bevy_ecs::prelude::*;
use npc_framework::agent_io::{ActionPlan, ActionRequest, AgentCtx, Rejection};
use npc_framework::kernel::commands::CommandError;
use npc_framework::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::json;

/// Reserve the scenario's visitor identity before play, without starting a life.
pub(super) fn reserve_visitor(world: &mut World, id: SimId, actor: Entity, person: Villager) {
    if id == world.resource::<VillageState>().played {
        world
            .entity_mut(actor)
            .remove::<(Villager, Vitality, Needs)>()
            .insert(AwaitingArrival {
                name: person.name,
                home: person.home,
            });
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JoinIsland {
    pub unit: u32,
}
impl JoinIsland {
    pub fn check(&self, world: &World) -> Result<(), String> {
        if world
            .get_resource::<VillageState>()
            .is_none_or(|v| v.played != SimId(self.unit))
        {
            return Err("only the island's reserved visitor may arrive".into());
        }
        let actor = world
            .resource::<SimIndex>()
            .get(SimId(self.unit))
            .ok_or("visitor identity is absent")?;
        if world.get::<AwaitingArrival>(actor).is_none() {
            return Err(
                "this visitor already began a life; arrival cannot reset or revive a character"
                    .into(),
            );
        }
        if world.get::<Vitality>(actor).is_some() || world.get::<Villager>(actor).is_some() {
            return Err("this visitor has already entered the simulation".into());
        }
        Ok(())
    }
    pub fn perform(&self, world: &mut World) -> Result<(), String> {
        self.check(world)?;
        let actor = world
            .resource::<SimIndex>()
            .get(SimId(self.unit))
            .expect("visitor");
        let pending = world
            .entity_mut(actor)
            .take::<AwaitingArrival>()
            .expect("checked arrival");
        super::super::initialise_person(
            world,
            actor,
            Role::Visitor,
            &pending.name,
            pending.home,
            SimId(0),
        );
        let house = home_of(world, SimId(self.unit));
        record_effect(
            world,
            Some(SimId(self.unit)),
            "VillageArrival",
            &json!({"life":world.resource::<VillageState>().life,"house":house}),
        );
        Ok(())
    }
}
impl ContentCommand for JoinIsland {
    const NAME: &'static str = "JoinIsland";
    fn validate(&self, world: &World) -> Result<(), CommandError> {
        self.check(world)
            .map_err(|reason| CommandError::Unavailable { reason })
    }
    fn apply(self, world: &mut World) {
        let _ = self.perform(world);
    }
    fn unit(&self) -> Option<SimId> {
        Some(SimId(self.unit))
    }
}
pub(super) fn join_handler(
    world: &World,
    ctx: &AgentCtx,
    request: &ActionRequest,
) -> Result<ActionPlan, Rejection> {
    if request.target.is_some()
        || request
            .args
            .as_object()
            .is_some_and(|args| !args.is_empty())
    {
        return Err(Rejection::new(
            "invalid_args",
            "join_island takes no target or arguments",
        ));
    }
    let command = JoinIsland { unit: ctx.unit.0 };
    command
        .check(world)
        .map_err(|reason| Rejection::new("unavailable", reason))?;
    Ok(ActionPlan::Commands {
        label: "Arrive on the island".into(),
        commands: vec![SimCommand::content(&command)],
    })
}
