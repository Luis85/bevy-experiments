//! Beds, ownership, hearths and commissions. Queries describe demand; AI executes work.
use super::super::{Role, Villager, economy, living};
use super::{ColonyState, Household};
use bevy_ecs::prelude::*;
use npc_framework::prelude::*;
use serde::Serialize;
use serde_json::json;

const HEARTH_FUEL_REQUIRED: u32 = 1;
const HEARTH_REACH_MM: u32 = 1500;

fn heating_period(world: &World) -> u64 {
    let day = world.resource::<SimConfig>().day_length;
    // Regional winters burn through a log faster: three lightings a day, not two.
    if let Some(tuning) = super::super::frontier::economy_spec::tuning(world)
        && super::super::calendar(world).1 == super::super::Season::Winter
    {
        let lightings = tuning
            .winter_hearth_lightings
            .saturating_add(super::super::weather::extra_hearth_lightings(world));
        return (day / u64::from(lightings.max(1))).max(1);
    }
    (day / 2).max(1)
}

/// Ticks one hearth log heats a home today: half a day, less in a regional
/// winter and less again in a harsh one.
pub fn heating_period_ticks(world: &World) -> u64 {
    heating_period(world)
}

/// Own-household facts and public hearth rules. No other resident's inventory is
/// exposed, and inspecting a hearth never lights it or reserves fuel.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct HomeHeating {
    pub heated: bool,
    pub remaining_ticks: u64,
    pub period_ticks: u64,
    pub cold: bool,
    pub fuel_kind: &'static str,
    pub fuel_required: u32,
    pub carried_fuel: u32,
    pub within_reach: bool,
    pub reach_mm: u32,
    pub ready_to_light: bool,
}

pub fn home_heating(world: &World, actor: Entity) -> Option<HomeHeating> {
    if !world.contains_resource::<ColonyState>() || !living(world, actor) {
        return None;
    }
    let id = world.get::<SimId>(actor)?;
    let home = home_of(world, *id)
        .and_then(|id| world.resource::<SimIndex>().get(id))
        .and_then(|entity| world.get::<Household>(entity))?;
    let remaining_ticks = home
        .heated_until
        .saturating_sub(world.resource::<SimClock>().tick);
    let heated = remaining_ticks > 0;
    let cold = super::super::living::cold(world);
    let carried_fuel = economy::count(world, actor, super::super::roles::roles(world).fuel);
    let within_reach = world
        .get::<SimPos>(actor)
        .is_some_and(|position| position.dist2(home.door) <= i64::from(HEARTH_REACH_MM).pow(2));
    Some(HomeHeating {
        heated,
        remaining_ticks,
        period_ticks: heating_period(world),
        cold,
        fuel_kind: super::super::roles::roles(world).fuel,
        fuel_required: HEARTH_FUEL_REQUIRED,
        carried_fuel,
        within_reach,
        reach_mm: HEARTH_REACH_MM,
        ready_to_light: cold && !heated && within_reach && carried_fuel >= HEARTH_FUEL_REQUIRED,
    })
}

pub(super) fn establish_resident_home(world: &mut World, id: SimId, person: &Villager) {
    // Regional construction assigns an independent house before colony startup.
    // Rebuilding household state on a workplace would erase that ownership.
    if home_of(world, id).is_some() {
        return;
    }
    if let Some(place) = world.resource::<SimIndex>().get(person.workplace) {
        world.entity_mut(place).insert(Household {
            label: format!("{} household", person.name),
            capacity: 2,
            residents: vec![id],
            door: person.home,
            heated_until: 0,
            logs_burned: 0,
        });
    }
}

/// Households ascending by identity: a query over the homes, not a scan of
/// every identity in the world.
pub(super) fn households(world: &World) -> Vec<(SimId, Entity)> {
    let Some(mut query) = world.try_query::<(Entity, &SimId, &Household)>() else {
        return Vec::new();
    };
    let mut out: Vec<(SimId, Entity)> = query.iter(world).map(|(e, id, _)| (*id, e)).collect();
    out.sort_unstable_by_key(|(id, _)| *id);
    out
}
/// The household with the lowest identity that lists `resident`.
pub fn home_of(world: &World, resident: SimId) -> Option<SimId> {
    let mut query = world.try_query::<(&SimId, &Household)>()?;
    query
        .iter(world)
        .filter(|(_, home)| home.residents.contains(&resident))
        .map(|(id, _)| *id)
        .min()
}
pub fn home_heated(world: &World, actor: Entity) -> bool {
    if !world.contains_resource::<ColonyState>() {
        return true;
    }
    world
        .get::<SimId>(actor)
        .and_then(|id| home_of(world, *id))
        .and_then(|id| world.resource::<SimIndex>().get(id))
        .and_then(|e| world.get::<Household>(e))
        .is_some_and(|home| home.heated_until > world.resource::<SimClock>().tick)
}

/// The first living NPC member supplies its shared hearth (or the remaining
/// visitor in a household without residents). This is a
/// financed inventory target, not a fuel grant or a second household inventory.
pub fn household_materials(world: &World, actor: Entity) -> Vec<(&'static str, u32)> {
    if !world.contains_resource::<ColonyState>() {
        return Vec::new();
    }
    let Some(id) = world.get::<SimId>(actor).copied() else {
        return Vec::new();
    };
    let Some(home) = home_of(world, id)
        .and_then(|id| world.resource::<SimIndex>().get(id))
        .and_then(|e| world.get::<Household>(e))
    else {
        return Vec::new();
    };
    let resident_npc = |id: &&SimId| {
        world.resource::<SimIndex>().get(**id).is_some_and(|e| {
            living(world, e)
                && world
                    .get::<Villager>(e)
                    .is_some_and(|v| v.role != Role::Visitor)
        })
    };
    let steward = home.residents.iter().find(resident_npc).or_else(|| {
        home.residents.iter().find(|id| {
            world
                .resource::<SimIndex>()
                .get(**id)
                .is_some_and(|e| living(world, e))
        })
    });
    let soon = world
        .resource::<SimClock>()
        .tick
        .saturating_add(world.resource::<SimConfig>().day_length / 4);
    if steward != Some(&id) {
        return Vec::new();
    }
    let lighting = if home.heated_until <= soon {
        HEARTH_FUEL_REQUIRED
    } else {
        0
    };
    // Heat demand reaches the woodcutters before the cold does: in the regional
    // profile a steward keeps a fuel reserve from a few days before winter until
    // it ends, instead of ordering one log when the hearth is already going out.
    // A home being built for people waiting for a bed comes first: while it
    // still lacks fuel logs, the reserve waits and only the hearth's own
    // need is ordered (closing fix).
    let fuel = super::super::roles::roles(world).fuel;
    let reserve = super::super::frontier::settlements::member(world, actor).map_or_else(
        || winter_fuel_reserve(world),
        |region| {
            if crate::village::civic::home_waits_for(world, region, fuel) {
                0
            } else {
                fuel_level_reserve(world, region, winter_fuel_reserve(world))
            }
        },
    );
    let wanted = lighting.max(reserve);
    if wanted > 0 {
        vec![(super::super::roles::roles(world).fuel, wanted)]
    } else {
        Vec::new()
    }
}

/// A settlement's fuel reserve after the overseer's fuel priority.
pub(crate) fn fuel_level_reserve(world: &World, settlement: SimId, normal: u32) -> u32 {
    use crate::village::frontier::oversee::{self, PriorityTopic};
    oversee::scale(
        oversee::level(world, settlement, PriorityTopic::FuelReserve, None),
        normal,
    )
}

/// Hearth logs a regional steward keeps in and shortly before winter.
fn winter_fuel_reserve(world: &World) -> u32 {
    let Some(tuning) = super::super::frontier::economy_spec::tuning(world) else {
        return 0;
    };
    let (day, season, _) = super::super::calendar(world);
    let season_days = super::super::days_per_season(world);
    let winter = |day: u64| (day / season_days) % 4 == 3;
    if season == super::super::Season::Winter
        || (1..=tuning.winter_fuel_lead_days).any(|ahead| winter(day + ahead))
    {
        tuning.winter_fuel_reserve
    } else {
        0
    }
}
pub(super) fn assign(world: &mut World, resident: SimId, house: SimId) {
    let home = world
        .resource::<SimIndex>()
        .get(house)
        .expect("checked home");
    let door = world.get::<Household>(home).expect("house").door;
    for (_, entity) in households(world) {
        world
            .get_mut::<Household>(entity)
            .expect("house")
            .residents
            .retain(|id| *id != resident);
    }
    let mut household = world.get_mut::<Household>(home).expect("house");
    household.residents.push(resident);
    household.residents.sort_unstable();
    let actor = world
        .resource::<SimIndex>()
        .get(resident)
        .expect("resident");
    let now = world.resource::<SimClock>().tick;
    let mut person = world.get_mut::<Villager>(actor).expect("resident");
    // Living world: a resident moved to another home while it rests in the
    // old one wakes, and walks to the new one: it does not sleep on where
    // the old home was, outside every building.
    if person.home != door && person.resting_until > now {
        person.resting_until = now;
    }
    person.home = door;
    record_effect(
        world,
        Some(resident),
        "VillageHomeAssigned",
        &json!({"house":house,"door":door}),
    );
}

/// Shared by first arrival and later recorded lives. Never resets existing
/// relationships, manufactures provisions, or revives a corpse.
pub(super) fn house_participant(world: &mut World, actor: Entity) {
    let Some(id) = world.get::<SimId>(actor).copied() else {
        return;
    };
    let Some(person) = world.get::<Villager>(actor) else {
        return;
    };
    let visitor = person.role == Role::Visitor;
    if visitor && home_of(world, id).is_none() {
        let house = households(world)
            .into_iter()
            .find(|(_, e)| {
                world
                    .get::<Household>(*e)
                    .is_some_and(|home| home.residents.len() < home.capacity as usize)
            })
            .map(|(id, _)| id);
        if let Some(house) = house {
            assign(world, id, house);
        }
    }
}

pub fn can_claim_cottage(world: &World, plot: Entity, claimant: SimId) -> Result<(), String> {
    if !world.contains_resource::<ColonyState>() {
        return Ok(());
    }
    let house = world
        .get::<super::super::homestead::HomesteadPlot>(plot)
        .and_then(|estate| estate.cottage_entity)
        .and_then(|id| world.resource::<SimIndex>().get(id))
        .and_then(|e| world.get::<Household>(e));
    if let Some(house) = house {
        let occupied = house
            .residents
            .iter()
            .filter(|id| {
                world
                    .resource::<SimIndex>()
                    .get(**id)
                    .is_some_and(|e| living(world, e))
            })
            .count();
        if !house.residents.contains(&claimant) && occupied >= house.capacity as usize {
            return Err(
                "the cottage has no free resident bed; reassign a household member first".into(),
            );
        }
    }
    Ok(())
}

/// Called only by successful claim/build effects. Later explicit household
/// assignments are not overwritten by background upkeep.
pub fn cottage_claimed(world: &mut World, plot: Entity, owner: SimId) {
    if !world.contains_resource::<ColonyState>() {
        return;
    }
    let Some(id) = world
        .get::<super::super::homestead::HomesteadPlot>(plot)
        .and_then(|p| p.cottage_entity)
    else {
        return;
    };
    let Some(cottage) = world.resource::<SimIndex>().get(id) else {
        return;
    };
    let Some(center) = world.get::<SimPos>(plot).copied() else {
        return;
    };
    if world.get::<Household>(cottage).is_none() {
        // Living world: the beds of the catalogue cottage; elsewhere three.
        let (_, _, capacity) = super::super::homestead::cottage_body(
            world,
            super::super::homestead::front_door(center),
            None,
        );
        world.entity_mut(cottage).insert(Household {
            label: "Homestead cottage".into(),
            capacity,
            residents: Vec::new(),
            door: super::super::homestead::front_door(center),
            heated_until: 0,
            logs_burned: 0,
        });
    }
    let alive: Vec<_> = world
        .get::<Household>(cottage)
        .expect("house")
        .residents
        .iter()
        .copied()
        .filter(|id| {
            world
                .resource::<SimIndex>()
                .get(*id)
                .is_some_and(|e| living(world, e))
        })
        .collect();
    world
        .get_mut::<Household>(cottage)
        .expect("house")
        .residents = alive;
    if world
        .resource::<SimIndex>()
        .get(owner)
        .is_some_and(|e| living(world, e))
        && can_claim_cottage(world, plot, owner).is_ok()
    {
        assign(world, owner, id);
    }
}

/// A real private procurement target, financed by the existing production plan.
/// Reserved building materials must never be requested a second time.
pub fn construction_materials(world: &World, actor: Entity) -> Vec<(&'static str, u32)> {
    let Some(state) = world.get_resource::<ColonyState>() else {
        return Vec::new();
    };
    let Some(id) = world.get::<SimId>(actor) else {
        return Vec::new();
    };
    if world
        .get::<super::super::homestead::CottageBuild>(actor)
        .is_some()
        || !state.projects.values().any(|p| p.builder == *id)
    {
        return Vec::new();
    }
    vec![
        (
            super::super::roles::roles(world).lumber,
            super::super::homestead::PLANK_COST,
        ),
        (
            super::super::roles::roles(world).timber,
            super::super::homestead::LOG_COST,
        ),
    ]
}

/// A read-only work request. The resident action layer owns movement, claims
/// and construction execution; this context only owns the commission.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConstructionIntent {
    InProgress,
    Build {
        builder: SimId,
        plot: SimId,
        door: SimPos,
        claim_required: bool,
    },
}

pub fn construction_intent(world: &World, actor: Entity) -> Option<ConstructionIntent> {
    let state = world.get_resource::<ColonyState>()?;
    let builder = *world.get::<SimId>(actor)?;
    let plot_id = state
        .projects
        .iter()
        .find(|(_, project)| project.builder == builder)
        .map(|(id, _)| *id)?;
    if world
        .get::<super::super::homestead::CottageBuild>(actor)
        .is_some()
    {
        return Some(ConstructionIntent::InProgress);
    }
    if construction_materials(world, actor)
        .iter()
        .any(|(kind, count)| economy::count(world, actor, kind) < *count)
    {
        return None;
    }
    let plot = world.resource::<SimIndex>().get(plot_id)?;
    let estate = world.get::<super::super::homestead::HomesteadPlot>(plot)?;
    if estate.cottage_built {
        return None;
    }
    let center = *world.get::<SimPos>(plot)?;
    Some(ConstructionIntent::Build {
        builder,
        plot: plot_id,
        door: super::super::homestead::front_door(center),
        claim_required: estate.owner != Some(builder),
    })
}

/// Stable-order household upkeep. The composition root handles demand invalidation.
pub(super) fn tick(world: &mut World, now: u64) -> bool {
    let mut demand_changed = false;
    for (house_id, house) in households(world) {
        let departed: Vec<_> = world
            .get::<Household>(house)
            .expect("home")
            .residents
            .iter()
            .copied()
            .filter(|id| {
                world
                    .resource::<SimIndex>()
                    .get(*id)
                    .is_none_or(|e| !living(world, e))
            })
            .collect();
        if !departed.is_empty() {
            world
                .get_mut::<Household>(house)
                .expect("home")
                .residents
                .retain(|id| !departed.contains(id));
            for resident in departed {
                record_effect(
                    world,
                    Some(resident),
                    "VillageBedVacated",
                    &json!({"house":house_id}),
                );
            }
        }
    }
    // Existing shared construction creates usable accommodation at completion.
    let cottages: Vec<_> = world
        .resource::<SimIndex>()
        .map
        .iter()
        .filter_map(|(id, entity)| {
            let cottage = world.get::<super::super::homestead::Cottage>(*entity)?;
            if world.get::<Household>(*entity).is_some() {
                return None;
            }
            let plot = world.resource::<SimIndex>().get(cottage.plot)?;
            let state = world.get::<super::super::homestead::HomesteadPlot>(plot)?;
            let owner = state.owner?;
            Some((
                *id,
                *entity,
                owner,
                super::super::homestead::front_door(*world.get::<SimPos>(plot)?),
            ))
        })
        .collect();
    for (id, entity, owner, door) in cottages {
        world.entity_mut(entity).insert(Household {
            label: "Homestead cottage".into(),
            capacity: 3,
            residents: Vec::new(),
            door,
            heated_until: 0,
            logs_burned: 0,
        });
        if world
            .resource::<SimIndex>()
            .get(owner)
            .is_some_and(|e| living(world, e))
        {
            assign(world, owner, id);
        }
    }
    let finished: Vec<_> = world
        .resource::<ColonyState>()
        .projects
        .iter()
        .filter_map(|(plot, project)| {
            let builder_alive = world
                .resource::<SimIndex>()
                .get(project.builder)
                .is_some_and(|e| living(world, e));
            let estate = world
                .resource::<SimIndex>()
                .get(*plot)
                .and_then(|e| world.get::<super::super::homestead::HomesteadPlot>(e));
            let complete = estate.is_some_and(|p| p.cottage_built);
            let taken = estate.is_some_and(|p| {
                p.owner.is_some_and(|id| {
                    id != project.builder
                        && world
                            .resource::<SimIndex>()
                            .get(id)
                            .is_some_and(|e| living(world, e))
                })
            });
            (!builder_alive || complete || taken || estate.is_none()).then_some((
                *plot,
                project.builder,
                complete,
            ))
        })
        .collect();
    for (plot, builder, complete) in finished {
        world.resource_mut::<ColonyState>().projects.remove(&plot);
        demand_changed = true;
        record_effect(
            world,
            Some(builder),
            "VillageHousingCommissionFinished",
            &json!({"plot":plot,"complete":complete}),
        );
    }
    if super::super::living::cold(world) {
        let fuels = super::super::roles::roles(world).fuels;
        for (house_id, entity) in households(world) {
            let house = world.get::<Household>(entity).expect("house").clone();
            if house.heated_until > now {
                continue;
            }
            for resident in &house.residents {
                let Some(actor) = world.resource::<SimIndex>().get(*resident) else {
                    continue;
                };
                if !living(world, actor)
                    || world
                        .get::<SimPos>(actor)
                        .is_none_or(|pos| pos.dist2(house.door) > i64::from(HEARTH_REACH_MM).pow(2))
                {
                    continue;
                }
                // Any good tagged `fuel`, the first one carried (S2.9).
                let Some(fuel) = fuels
                    .iter()
                    .copied()
                    .find(|fuel| economy::count(world, actor, fuel) >= HEARTH_FUEL_REQUIRED)
                else {
                    continue;
                };
                if economy::consume(world, actor, fuel, HEARTH_FUEL_REQUIRED).is_ok() {
                    crate::village::ledger::used(world, fuel, HEARTH_FUEL_REQUIRED);
                    let heated_until = now.saturating_add(heating_period(world));
                    let mut house = world.get_mut::<Household>(entity).expect("house");
                    house.heated_until = heated_until;
                    house.logs_burned = house
                        .logs_burned
                        .saturating_add(u64::from(HEARTH_FUEL_REQUIRED));
                    record_effect(
                        world,
                        Some(*resident),
                        "VillageHearthLit",
                        &json!({"house":house_id,"logs":HEARTH_FUEL_REQUIRED,"heated_until":heated_until}),
                    );
                    break;
                }
            }
        }
    }
    demand_changed
}
