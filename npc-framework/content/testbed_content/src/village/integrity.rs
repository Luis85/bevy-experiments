//! Read-only integrity boundary for hosts restoring a complete colony.
//! The content context owns these invariants; hosts own files and recovery UX.
use super::{
    Role, VillagePlace, VillageState, Villager, Vitality, Wallet,
    colony::{AwaitingArrival, ColonyState, Household, SocialLife},
    homestead::{Cottage, HomesteadPlot},
    policy::IslandAiPolicy,
    terrain::{MAX_HEIGHT_MM, TerrainState, TerrainSurface, WIDTH},
};
use bevy_ecs::prelude::*;
use npc_framework::prelude::*;
use std::collections::BTreeSet;

/// Required construction identities may become corpses or acquire new state,
/// but a truncated file must not silently remove a resident, worksite or plot.
pub fn validate_snapshot_structure(
    expected: &npc_framework::snapshot::WorldSnapshot,
    saved: &npc_framework::snapshot::WorldSnapshot,
) -> Result<(), String> {
    for name in expected.resources.keys() {
        if !saved.resources.contains_key(name) {
            return Err(format!("Required resource {name} is missing"));
        }
    }
    for baseline in &expected.entities {
        let required: Vec<_> = [
            Villager::NAME,
            VillagePlace::NAME,
            super::ResourcePatch::NAME,
            HomesteadPlot::NAME,
            super::hunting::WildlifeHabitat::NAME,
            Household::NAME,
            super::frontier::labor::ResidentProfile::NAME,
            super::frontier::labor::Employment::NAME,
            super::frontier::Settlement::NAME,
            super::frontier::SettlementMember::NAME,
            super::frontier::SettlementSite::NAME,
            super::frontier::ecology::EcologicalPatch::NAME,
            super::frontier::ecology::HabitatEcology::NAME,
        ]
        .into_iter()
        .filter(|name| baseline.components.contains_key(*name))
        .collect();
        let arrival = baseline.components.contains_key(AwaitingArrival::NAME);
        if required.is_empty() && !arrival {
            continue;
        }
        let Some(entity) = saved
            .entities
            .iter()
            .find(|entity| entity.id == baseline.id)
        else {
            if mortal_and_gone(baseline, saved) {
                continue;
            }
            return Err(format!(
                "Required island identity {} is missing",
                baseline.id
            ));
        };
        for name in required {
            if !entity.components.contains_key(name) {
                return Err(format!("Island identity {} is missing {name}", baseline.id));
            }
        }
        if let Some(identity) = baseline
            .components
            .get(super::frontier::labor::ResidentProfile::NAME)
            && entity
                .components
                .get(super::frontier::labor::ResidentProfile::NAME)
                != Some(identity)
        {
            return Err(format!(
                "Regional identity {} differs from its frozen construction profile",
                baseline.id
            ));
        }
        if arrival
            && !entity.components.contains_key(AwaitingArrival::NAME)
            && !entity.components.contains_key(Villager::NAME)
        {
            return Err("The initial visitor has neither an arrival nor a mortal life".into());
        }
    }
    Ok(())
}

/// Whether a construction identity may be missing from a later save, and is
/// accounted for. Permanent identities (settlements, their founding sites,
/// workplaces and homes, homestead plots, wildlife habitats, the arrival)
/// must always be there. Mortal ones may go, each only the way the
/// simulation removes it:
///
/// - an animal (`island_wildlife`): hunted, or died in its habitat;
/// - a resident: laid to rest, so `Demography.departed` names it;
/// - a resource patch: removed by `RemoveResourcePatch`, which never removes
///   the last patch of a good in its settlement, so another one remains;
/// - a building site laid out from an exported world: cancelled (a finished
///   one keeps its identity as the building).
fn mortal_and_gone(
    baseline: &npc_framework::snapshot::EntitySnapshot,
    saved: &npc_framework::snapshot::WorldSnapshot,
) -> bool {
    use serde_json::Value;
    let has = |name: &str| baseline.components.contains_key(name);
    let permanent = [
        super::frontier::Settlement::NAME,
        super::frontier::SettlementSite::NAME,
        VillagePlace::NAME,
        Household::NAME,
        HomesteadPlot::NAME,
        super::hunting::WildlifeHabitat::NAME,
        super::frontier::ecology::HabitatEcology::NAME,
        AwaitingArrival::NAME,
    ];
    if permanent.iter().any(|name| has(name)) {
        return false;
    }
    if has(super::hunting::Wildlife::NAME) || has(super::civic::ConstructionSite::NAME) {
        return true;
    }
    if has(super::frontier::labor::ResidentProfile::NAME) || has(Villager::NAME) {
        return saved
            .resources
            .get(super::demography::Demography::NAME)
            .and_then(|demography| demography.get("departed"))
            .and_then(Value::as_array)
            .is_some_and(|departed| {
                departed.iter().any(|record| {
                    record.get("resident").and_then(Value::as_u64) == Some(baseline.id)
                })
            });
    }
    if let Some(patch) = baseline.components.get(super::ResourcePatch::NAME) {
        let member = |components: &std::collections::BTreeMap<String, Value>| {
            components
                .get(super::frontier::SettlementMember::NAME)
                .cloned()
        };
        let kind = patch.get("kind").cloned();
        let settlement = member(&baseline.components);
        return kind.is_some()
            && saved.entities.iter().any(|other| {
                other
                    .components
                    .get(super::ResourcePatch::NAME)
                    .is_some_and(|other_patch| other_patch.get("kind").cloned() == kind)
                    && member(&other.components) == settlement
            });
    }
    false
}

pub fn validate_colony(world: &World) -> Result<(), String> {
    if !super::colony::enabled(world) {
        return Err("This save does not describe an island colony".into());
    }
    let index = world
        .get_resource::<SimIndex>()
        .ok_or("Island identities are missing")?;
    let state = world
        .get_resource::<VillageState>()
        .ok_or("Island state is missing")?;
    let ids = world
        .get_resource::<crate::TestbedIds>()
        .ok_or("Island anchors are missing")?;
    let played = index
        .get(state.played)
        .ok_or("The island's played identity is missing")?;
    if world.get::<SimPos>(played).is_none() {
        return Err("The played identity has no position".into());
    }
    if world.get::<AwaitingArrival>(played).is_some() {
        if world.get::<Villager>(played).is_some()
            || world.get::<Vitality>(played).is_some()
            || world.get::<Needs>(played).is_some()
        {
            return Err("A reserved arrival cannot already have a mortal life".into());
        }
    } else if world
        .get::<Villager>(played)
        .is_none_or(|person| person.role != Role::Visitor)
    {
        return Err("The played identity has no visitor life or reserved arrival".into());
    }
    if super::frontier::enabled(world) {
        super::frontier::settlements::validate(world)?;
        super::frontier::ecology::validate(world)?;
        super::frontier::labor::validate(world)?;
        super::civic::validate(world)?;
    } else {
        for role in Role::RESIDENTS
            .into_iter()
            .chain([Role::Hunter, Role::Tanner, Role::Tailor])
        {
            let entity = ids
                .get(role.person())
                .and_then(|id| index.get(id))
                .ok_or_else(|| format!("Required resident {} is missing", role.person()))?;
            if world
                .get::<Villager>(entity)
                .is_none_or(|person| person.role != role)
            {
                return Err(format!(
                    "Required resident {} has no matching life",
                    role.person()
                ));
            }
        }
    }
    let mut members = BTreeSet::new();
    for (id, entity) in &index.map {
        if let Some(work) = world.get::<super::frontier::activities::EcologyWork>(*entity) {
            if !super::frontier::enabled(world)
                || !super::living(world, *entity)
                || world.get::<AwaitingArrival>(*entity).is_some()
            {
                return Err("Restoration work requires a living regional participant".into());
            }
            super::frontier::activities::validate_work(world, *entity, work)?;
        }
        if let Some(person) = world.get::<Villager>(*entity) {
            if world.get::<Vitality>(*entity).is_none()
                || world.get::<Needs>(*entity).is_none()
                || world.get::<Wallet>(*entity).is_none()
                || world.get::<Inventory>(*entity).is_none()
                || world.get::<SimPos>(*entity).is_none()
                || world.get::<SocialLife>(*entity).is_none()
            {
                return Err(format!("Resident {} has incomplete life state", id.0));
            }
            if person.role != Role::Visitor
                && index
                    .get(person.workplace)
                    .and_then(|workplace| world.get::<VillagePlace>(workplace))
                    .is_none()
            {
                return Err(format!("Resident {} has a missing workplace", id.0));
            }
        }
        if let Some(house) = world.get::<Household>(*entity) {
            if house.capacity == 0
                || house.residents.len() > house.capacity as usize
                || house.residents.windows(2).any(|pair| pair[0] >= pair[1])
            {
                return Err(format!(
                    "Household {} has invalid capacity or membership",
                    id.0
                ));
            }
            for member in &house.residents {
                let person = index
                    .get(*member)
                    .and_then(|entity| world.get::<Villager>(entity))
                    .ok_or("A household refers to a missing resident")?;
                if !members.insert(*member) || person.home != house.door {
                    return Err("A resident has inconsistent household membership".into());
                }
            }
        }
        if let Some(plot) = world.get::<HomesteadPlot>(*entity)
            && (plot.owner.is_some_and(|owner| {
                index
                    .get(owner)
                    .and_then(|e| world.get::<Villager>(e))
                    .is_none()
            }) || plot.cottage_entity.is_some_and(|cottage| {
                index
                    .get(cottage)
                    .and_then(|e| world.get::<Cottage>(e))
                    .is_none()
            }))
        {
            return Err("A homestead refers to a missing owner or cottage".into());
        }
        if let Some(cottage) = world.get::<Cottage>(*entity)
            && index
                .get(cottage.plot)
                .and_then(|plot| world.get::<HomesteadPlot>(plot))
                .is_none_or(|plot| !plot.cottage_built || plot.cottage_entity != Some(*id))
        {
            return Err("A cottage has no matching completed homestead".into());
        }
    }
    let colony = world
        .get_resource::<ColonyState>()
        .ok_or("Colony state is missing")?;
    if !(1..=4).contains(&colony.food_reserve_days) {
        return Err("Colony food reserve policy is invalid".into());
    }
    for (plot, project) in &colony.projects {
        if index
            .get(*plot)
            .and_then(|e| world.get::<HomesteadPlot>(e))
            .is_none()
            || index
                .get(project.builder)
                .and_then(|e| world.get::<Villager>(e))
                .is_none()
        {
            return Err("A housing project has missing participants".into());
        }
    }
    if super::frontier::enabled(world) {
        super::frontier::terrain::validate(world)?;
    } else {
        let terrain = world
            .get_resource::<TerrainState>()
            .ok_or("Island terrain is missing")?;
        if terrain.cells.len() != WIDTH * WIDTH {
            return Err("Island terrain has an invalid size".into());
        }
        let grid = world
            .get_resource::<SpatialGrid>()
            .ok_or("Island navigation is missing")?;
        for z in -32..32 {
            for x in -32..32 {
                let cell = terrain.cell(x, z).expect("validated grid size");
                if !(-1000..=MAX_HEIGHT_MM).contains(&cell.height_mm)
                    || (cell.surface == TerrainSurface::Water && cell.height_mm > 0)
                    || (cell.surface != TerrainSurface::Water && cell.height_mm < 0)
                    || grid.static_cell(CellCoord::new(x, z)) != cell.navigation()
                {
                    return Err("Island terrain and navigation are inconsistent".into());
                }
            }
        }
    }
    if world
        .resource::<crate::world_runtime::ActiveWorld>()
        .0
        .island_ai
        .is_some()
    {
        world
            .get_resource::<IslandAiPolicy>()
            .ok_or("Resident AI policy is missing")?
            .validate()?;
    }
    Ok(())
}
