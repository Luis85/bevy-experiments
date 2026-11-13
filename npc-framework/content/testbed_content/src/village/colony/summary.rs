//! Read-only projections for inspectors and tools; never performs an action.
use super::super::{Villager, catalog, economy, living};
use super::{ColonyState, Household, SocialLife};
use bevy_ecs::prelude::*;
use npc_framework::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ColonySummary {
    pub residents: u32,
    pub housed: u32,
    pub beds: u32,
    pub households: u32,
    pub average_morale: i32,
    pub social_events: u64,
    pub food_units: u32,
    pub water_units: u32,
    pub construction_projects: u32,
}
pub fn summary(world: &World) -> ColonySummary {
    let mut result = ColonySummary::default();
    let mut morale = 0_i64;
    let mut morale_count = 0;
    for entity in world.resource::<SimIndex>().map.values() {
        if let Some(house) = world.get::<Household>(*entity) {
            result.households += 1;
            result.beds += house.capacity;
            result.housed += house
                .residents
                .iter()
                .filter(|id| {
                    world
                        .resource::<SimIndex>()
                        .get(**id)
                        .is_some_and(|e| living(world, e))
                })
                .count() as u32;
        }
        if world.get::<Villager>(*entity).is_some() && living(world, *entity) {
            result.residents += 1;
            for (_, item) in economy::free_items(world, *entity) {
                if catalog::food_restore_in(world, &item.kind).is_some() {
                    result.food_units = result.food_units.saturating_add(item.stack);
                }
                if item.kind == super::super::roles::roles(world).drink {
                    result.water_units = result.water_units.saturating_add(item.stack);
                }
            }
            if let Some(social) = world.get::<SocialLife>(*entity) {
                morale += i64::from(social.morale);
                morale_count += 1;
            }
        }
    }
    if morale_count > 0 {
        result.average_morale = (morale / morale_count) as i32;
    }
    result.social_events = world
        .get_resource::<ColonyState>()
        .map_or(0, |state| state.social_events);
    result.construction_projects = world
        .get_resource::<ColonyState>()
        .map_or(0, |state| state.projects.len() as u32);
    result
}
