//! Recorded colony input adapter: validation, domain changes and explicit island glue.
use super::super::{Role, Villager, living};
use super::{ColonyState, Household, HousingProject, housing::assign};
use bevy_ecs::prelude::*;
use npc_framework::kernel::commands::CommandError;
use npc_framework::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ColonyAct {
    pub verb: String,
    #[serde(default)]
    pub resident: Option<SimId>,
    #[serde(default)]
    pub house: Option<SimId>,
    #[serde(default)]
    pub value: u32,
}
impl ColonyAct {
    pub fn check(&self, world: &World) -> Result<(), String> {
        if !world.contains_resource::<ColonyState>() {
            return Err("colony controls require the island colony".into());
        }
        match self.verb.as_str() {
            "reserve_food" | "gatherings" => {
                if self.resident.is_some() || self.house.is_some() {
                    return Err("policy controls do not take a resident or house".into());
                }
                if self.verb == "reserve_food" && !(1..=4).contains(&self.value) {
                    return Err("food reserve must be 1..4 days".into());
                }
                if self.verb == "gatherings" && self.value > 1 {
                    return Err("gatherings must be 0 or 1".into());
                }
                Ok(())
            }
            "assign_home" => {
                if self.value != 0 {
                    return Err("assign_home does not take a policy value".into());
                }
                let resident = self.resident.ok_or("choose a resident")?;
                let actor = world
                    .resource::<SimIndex>()
                    .get(resident)
                    .ok_or("resident no longer exists")?;
                if !living(world, actor) || world.get::<Villager>(actor).is_none() {
                    return Err("only a living participant can be housed".into());
                }
                let house = self
                    .house
                    .and_then(|id| world.resource::<SimIndex>().get(id))
                    .and_then(|e| world.get::<Household>(e))
                    .ok_or("choose a household")?;
                if !house.residents.contains(&resident)
                    && house.residents.len() >= house.capacity as usize
                {
                    return Err("that household has no empty bed".into());
                }
                Ok(())
            }
            "commission_home" => {
                if self.value != 0 {
                    return Err("commission_home does not take a policy value".into());
                }
                let builder = self.resident.ok_or("choose an NPC builder")?;
                let actor = world
                    .resource::<SimIndex>()
                    .get(builder)
                    .ok_or("builder no longer exists")?;
                if !living(world, actor)
                    || world
                        .get::<Villager>(actor)
                        .is_none_or(|v| v.role == Role::Visitor)
                {
                    return Err("choose a living NPC resident; visitors build through their own homestead actions".into());
                }
                let plot_id = self.house.ok_or("choose a vacant housing plot")?;
                let plot = world
                    .resource::<SimIndex>()
                    .get(plot_id)
                    .ok_or("plot no longer exists")?;
                let state = world
                    .get::<super::super::homestead::HomesteadPlot>(plot)
                    .ok_or("commission requires a homestead plot")?;
                if super::super::homestead::definition(world, plot)
                    .is_none_or(|p| p.use_kind != "housing")
                    || state.cottage_built
                {
                    return Err("choose an unfinished housing plot".into());
                }
                if state.owner.is_some_and(|id| {
                    id != builder
                        && world
                            .resource::<SimIndex>()
                            .get(id)
                            .is_some_and(|e| living(world, e))
                }) {
                    return Err("that plot already has a living owner".into());
                }
                let projects = &world.resource::<ColonyState>().projects;
                if projects.contains_key(&plot_id)
                    || projects.values().any(|p| p.builder == builder)
                {
                    return Err("the plot or builder already has a housing commission".into());
                }
                if world.resource::<Reservations>().count(plot_id) > 0 {
                    return Err("the plot has work in progress".into());
                }
                if super::super::homestead::plots(world)
                    .into_iter()
                    .any(|(id, e)| {
                        id != plot_id
                            && world
                                .get::<super::super::homestead::HomesteadPlot>(e)
                                .is_some_and(|p| p.owner == Some(builder))
                            && super::super::homestead::definition(world, e)
                                .is_some_and(|p| p.use_kind == "housing")
                    })
                {
                    return Err("the builder already owns a housing plot".into());
                }
                Ok(())
            }
            "cancel_home" => {
                if self.value != 0 || self.resident.is_some() {
                    return Err("cancel_home requires only a plot".into());
                }
                if !self.house.is_some_and(|plot| {
                    world.resource::<ColonyState>().projects.contains_key(&plot)
                }) {
                    return Err("that plot has no pending commission".into());
                }
                Ok(())
            }
            _ => Err("unknown colony action".into()),
        }
    }
    pub fn perform(&self, world: &mut World) -> Result<(), String> {
        self.check(world)?;
        match self.verb.as_str() {
            "reserve_food" => {
                world.resource_mut::<ColonyState>().food_reserve_days = self.value;
                super::super::colony_demand_changed(world);
            }
            "gatherings" => world.resource_mut::<ColonyState>().gatherings = self.value == 1,
            "assign_home" => assign(
                world,
                self.resident.expect("checked resident"),
                self.house.expect("checked house"),
            ),
            "commission_home" => {
                let ordered_at = world.resource::<SimClock>().tick;
                world.resource_mut::<ColonyState>().projects.insert(
                    self.house.expect("checked plot"),
                    HousingProject {
                        builder: self.resident.expect("checked builder"),
                        ordered_at,
                    },
                );
                super::super::colony_demand_changed(world);
            }
            "cancel_home" => {
                let project = world
                    .resource_mut::<ColonyState>()
                    .projects
                    .remove(&self.house.expect("checked plot"))
                    .expect("checked project");
                if let Some(builder) = world.resource::<SimIndex>().get(project.builder) {
                    super::super::homestead::cancel(world, builder);
                }
                super::super::colony_demand_changed(world);
            }
            _ => unreachable!("checked action"),
        }
        record_effect(
            world,
            self.resident,
            "VillageColonyPolicy",
            &json!({"verb":self.verb,"value":self.value,"house":self.house}),
        );
        Ok(())
    }
}
impl ContentCommand for ColonyAct {
    const NAME: &'static str = "ColonyAct";
    fn validate(&self, world: &World) -> Result<(), CommandError> {
        self.check(world)
            .map_err(|reason| CommandError::Unavailable { reason })
    }
    fn apply(self, world: &mut World) {
        let _ = self.perform(world);
    }
}
