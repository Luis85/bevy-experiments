//! Public, read-only observer projection. Call between ticks; never load this as a save.
//! Only island content is supported in v1. Host files, credentials and notebooks are excluded.
use bevy_ecs::prelude::*;
use npc_framework::interaction::{CarriedBy, Inventory, Item};
use npc_framework::prelude::*;
use serde_json::{Value, json};

use crate::village::{self, ResourcePatch, VillagePlace, VillageState, Villager, Vitality, Wallet};

/// Integer millimetres, x east / z south. SimIds are sorted; JSON is a full replacement.
pub fn world_state(world: &mut World) -> Result<Value, String> {
    let state = world
        .get_resource::<VillageState>()
        .ok_or("world projection v1 supports the island village only")?
        .clone();
    let (day, season, night) = village::calendar(world);
    let mut ids: Vec<_> = world
        .query::<(&SimId, Entity)>()
        .iter(world)
        .map(|(id, e)| (*id, e))
        .collect();
    ids.sort_by_key(|(id, _)| *id);
    let mut feedback = village::public_feedback::bubbles(world);
    let mut entities = Vec::new();
    for (id, e) in ids {
        let Some(pos) = world.get::<SimPos>(e) else {
            continue;
        };
        let mut view = json!({"id":id.0,"position":pos});
        if let Some(person) = world.get::<Villager>(e) {
            view["kind"] = json!("person");
            view["name"] = json!(person.name);
            view["role"] = json!(person.role);
            view["home"] = json!(person.home);
            view["workplace"] = json!(person.workplace.0);
            view["is_player"] = json!(id == state.played);
            view["activity"] = json!(village::activity(world, e).0);
            view["decision"] = json!(
                village::production::job_summary(world, e)
                    .or_else(|| village::behavior::decision(world, e).map(str::to_owned))
            );
            view["bubbles"] = json!(feedback.remove(&id).unwrap_or_default());
            view["wearing_warm_clothes"] = json!(village::living::has_coat(world, e));
            view["health"] = json!(world.get::<Vitality>(e).map_or(0, |v| v.health));
            view["coins"] = json!(world.get::<Wallet>(e).map_or(0, |w| w.coins));
            view["needs"] = npc_framework::needs::needs_json(world, e).unwrap_or(json!({}));
            let inventory: Vec<_> = world
                .get::<Inventory>(e)
                .into_iter()
                .flat_map(|i| &i.items)
                .filter_map(|id| {
                    let item = world
                        .resource::<SimIndex>()
                        .get(*id)
                        .and_then(|e| world.get::<Item>(e))?;
                    Some(json!({"id":id.0,"kind":item.kind,"count":item.stack}))
                })
                .collect();
            view["inventory"] = json!(inventory);
        } else if world.get::<village::homestead::HomesteadPlot>(e).is_some() {
            if let Some(plot) = village::homestead::plot_json(world, e, true) {
                entities.push(plot);
            }
            continue;
        } else if let Some(cottage) = world.get::<village::homestead::Cottage>(e) {
            let owner = world
                .resource::<SimIndex>()
                .get(cottage.plot)
                .and_then(|plot| world.get::<village::homestead::HomesteadPlot>(plot))
                .and_then(|plot| plot.owner);
            let owner_name = owner
                .and_then(|id| world.resource::<SimIndex>().get(id))
                .and_then(|owner| world.get::<Villager>(owner))
                .map(|v| v.name.as_str());
            view["kind"] = json!("cottage");
            view["name"] = json!(
                owner_name.map_or_else(|| "Cottage".to_owned(), |name| format!("{name}'s cottage"))
            );
            view["plot_id"] = json!(cottage.plot_id);
            view["owner"] = json!(owner);
            view["owner_name"] = json!(owner_name);
        } else if let Some(place) = world.get::<VillagePlace>(e) {
            view["kind"] = json!("building");
            view["name"] = json!(place.label);
            view["role"] = json!(place.role);
            view["recipes"] = json!(village::environment::station_recipes(world, e));
        } else if let Some(patch) = world.get::<ResourcePatch>(e) {
            view["kind"] = json!("resource");
            view["name"] = json!(patch.kind);
            view["remaining"] = json!(patch.remaining);
            view["capacity"] = json!(patch.capacity);
            view["replenish_at"] = json!(patch.replenish_at);
            view["gather_ticks"] = json!(patch.gather_ticks);
        } else if let Some(animal) = world.get::<village::hunting::Wildlife>(e) {
            view["kind"] = json!("resource");
            view["name"] = json!("deer");
            view["remaining"] = json!(u32::from(village::living(world, e)));
            view["capacity"] = json!(1);
            view["gather_ticks"] = json!(village::hunting::HUNT_TICKS);
            if let Some(renew_at) = world
                .resource::<SimIndex>()
                .get(animal.habitat)
                .and_then(|habitat| world.get::<village::hunting::WildlifeHabitat>(habitat))
                .map(|habitat| habitat.renew_at)
            {
                view["replenish_at"] = json!(renew_at);
            }
        } else if let Some(settlement) = world.get::<village::frontier::Settlement>(e) {
            view["kind"] = json!("settlement");
            view["name"] = json!(settlement.label);
        } else if village::frontier::enabled(world)
            && world.get::<village::colony::Household>(e).is_some()
        {
            let home = world.get::<village::colony::Household>(e).expect("home");
            view["kind"] = json!("home");
            view["name"] = json!(home.label);
            view["residents"] = json!(home.residents);
        } else if let Some(item) = world.get::<Item>(e) {
            if world.get::<CarriedBy>(e).is_some() {
                continue;
            }
            // Do not publish diary contents, even when a diary is lying on the ground.
            view["kind"] = json!("item");
            view["name"] = json!(item.kind);
            view["count"] = json!(item.stack);
        } else {
            continue;
        }
        entities.push(view);
    }
    let geography = *village::geometry::for_world(world);
    let cells: Vec<_> = (-32..32)
        .flat_map(|z| {
            (-32..32).map(move |x| {
                if geography.land_cell(x, z) {
                    1_u8
                } else {
                    0_u8
                }
            })
        })
        .collect();
    let mut terrain = json!({"origin":{"x":-32000,"z":-32000},"cell_size_mm":1000,
        "width":64,"height":64,"cells":cells,"paths":geography.roads});
    if let Some(regional) = world.get_resource::<village::frontier::terrain::TerrainState>() {
        let bounds = regional.bounds();
        terrain = json!({"origin":{"x":bounds.min.x*1000,"z":bounds.min.z*1000},"cell_size_mm":1000,
            "width":regional.width,"height":regional.height,
            "cells":regional.cells.iter().map(|c|u8::from(c.walkable())).collect::<Vec<_>>(),
            "height_mm":regional.cells.iter().map(|c|c.height_mm).collect::<Vec<_>>(),
            "revision":regional.revision});
    }
    if !geography.expansion_plots.is_empty() {
        terrain["expansion_plots"] = json!(geography.expansion_plots);
        terrain["districts"] = json!(geography.districts);
    }
    if let Some(spec) = village::frontier::spec(world) {
        terrain["expansion_plots"] = json!(spec.plots);
        terrain["districts"]=json!(spec.settlements.iter().map(|settlement|json!({
            "id":settlement.key,"label":settlement.label,"x":settlement.center.x,"z":settlement.center.z,"biome":settlement.biome
        })).collect::<Vec<_>>());
    }
    let world_name = world
        .get_resource::<crate::world_runtime::ActiveWorld>()
        .map(|active| active.0.name.as_str())
        .unwrap_or("Island Village");
    Ok(json!({
        "schema_version":1,
        "tick":world.resource::<SimClock>().tick,
        "world":{"id":if village::needs_driven(world){"living_island"}else{"island_village"},"name":world_name,"scene":"island_village",
            "economy":if village::needs_driven(world){"needs_driven"}else{"fixed"},
            "units":"mm","day":day,"season":season,"night":night,
            "day_length_ticks":world.resource::<SimConfig>().day_length},
        "terrain":terrain,
        "entities":entities,
        "market":village::market::public_book(world)
    }))
}
