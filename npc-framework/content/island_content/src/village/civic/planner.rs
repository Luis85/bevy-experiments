//! The civic planner: one slow, staggered review per settlement. It houses
//! the homeless, and when a shortage lasts it commissions a building, paid
//! from the civic fund through the site's open material orders: at most one
//! a review and one per need, and no more at once than the settlement can
//! staff and supply (`commission`).
use super::model::{CivicState, ConstructionSite, Depot};
use super::placement::{self, Survey};
use super::{depot, sites};
use crate::village::frontier::building_spec::{BuildingDef, BuildingKind};
use crate::village::frontier::labor::{self, ResidentProfile};
use crate::village::frontier::{Settlement, economy_spec, settlements};
use crate::village::{
    ResourcePatch, Role, VillagePlace, VillageState, catalog, colony, living, market,
};
use bevy_ecs::prelude::*;
use npc_framework::prelude::*;
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};

/// The planner leaves this many cells around the centre open as the square.
const SQUARE_CELLS: i32 = 8;

/// Adds a planner for every settlement that has none; call after construction.
pub(super) fn spawn(world: &mut World) {
    let ids: Vec<SimId> = settlements::all(world)
        .into_iter()
        .map(|(id, _)| id)
        .collect();
    let mut civic = world.resource_mut::<CivicState>();
    for id in ids {
        civic.planners.entry(id).or_default();
    }
}

/// Runs the review of each settlement whose turn it is. Settlements take
/// turns spread evenly over the review interval, so one tick reviews at most
/// one settlement and the cost per tick does not grow with the village.
pub(super) fn tick(world: &mut World, now: u64) {
    let Some(interval) = economy_spec::tuning(world).map(|tuning| tuning.planner_review_ticks)
    else {
        return;
    };
    if now == 0 {
        return;
    }
    let due: Vec<SimId> = {
        let civic = world.resource::<CivicState>();
        let count = civic.planners.len() as u64;
        civic
            .planners
            .keys()
            .enumerate()
            .filter(|(index, _)| now % interval == *index as u64 * interval / count.max(1))
            .map(|(_, id)| *id)
            .collect()
    };
    for settlement in due {
        review(world, settlement, now);
    }
}

struct Census {
    /// Living residents of the settlement without a bed.
    homeless: Vec<SimId>,
    /// Households of the settlement with a free bed: `(house, free beds)`.
    vacancies: Vec<(SimId, u32)>,
    members: Vec<Entity>,
}

fn census(world: &World, settlement: SimId) -> Census {
    let index = world.resource::<SimIndex>();
    let mut housed = BTreeSet::new();
    let mut vacancies = Vec::new();
    let mut members = Vec::new();
    for (id, entity) in &index.map {
        if let Some(home) = world.get::<colony::Household>(*entity) {
            let alive = home
                .residents
                .iter()
                .filter(|resident| index.get(**resident).is_some_and(|e| living(world, e)))
                .count() as u32;
            housed.extend(home.residents.iter().copied());
            // Children sleep in a bed too.
            let used = alive + crate::village::demography::children_of(world, *id).len() as u32;
            if settlements::member(world, *entity) == Some(settlement) && used < home.capacity {
                vacancies.push((*id, home.capacity - used));
            }
        }
        if world.get::<ResidentProfile>(*entity).is_some()
            && settlements::member(world, *entity) == Some(settlement)
            && living(world, *entity)
        {
            members.push(*entity);
        }
    }
    let homeless = members
        .iter()
        .filter_map(|entity| world.get::<SimId>(*entity).copied())
        .filter(|id| !housed.contains(id))
        .collect();
    Census {
        homeless,
        vacancies,
        members,
    }
}

/// Gives every homeless resident of the settlement a free bed there, in
/// identity order, through the ordinary recorded assignment. Returns the
/// residents still without a bed and the beds still free.
pub fn house_homeless(world: &mut World, settlement: SimId) -> (u32, u32) {
    let Census {
        homeless,
        mut vacancies,
        ..
    } = census(world, settlement);
    let mut left = 0;
    for resident in homeless {
        let Some(slot) = vacancies.iter_mut().find(|(_, free)| *free > 0) else {
            left += 1;
            continue;
        };
        let moved = colony::ColonyAct {
            verb: "assign_home".into(),
            resident: Some(resident),
            house: Some(slot.0),
            value: 0,
        }
        .perform(world)
        .is_ok();
        if moved {
            slot.1 -= 1;
        } else {
            left += 1;
        }
    }
    (left, vacancies.iter().map(|(_, free)| *free).sum())
}

/// Surplus the residents hold beyond their working stock that no store of
/// the settlement has room for. A store that takes only some goods (a
/// field's granary) is room only for the surplus of those goods: counted as
/// room for anything, the granaries hid the overflow of every other good
/// from the planner (a second crop's granary added 60 units of such room a
/// village, and the stockpiles it would have built went unbuilt).
fn unstored_surplus(world: &World, settlement: SimId, members: &[Entity]) -> u32 {
    let working = economy_spec::tuning(world).map_or(2, |tuning| tuning.depot_working_stock);
    let mut surplus: BTreeMap<&str, u32> = BTreeMap::new();
    for actor in members {
        for kind in catalog::trade_kinds(world) {
            if depot::storable(world, kind) {
                let units = market::available_to_sell(world, *actor, kind).saturating_sub(working);
                if units > 0 {
                    let entry = surplus.entry(kind).or_default();
                    *entry = entry.saturating_add(units);
                }
            }
        }
    }
    let mut room = 0_u32;
    for (_, entity) in depot::depots(world, settlement) {
        let Some(store) = world.get::<Depot>(entity) else {
            continue;
        };
        if store.only.is_empty() {
            room = room.saturating_add(store.free());
            continue;
        }
        // A restricted store absorbs the surplus of its own goods only.
        let mut free = store.free();
        for kind in &store.only {
            if let Some(units) = surplus.get_mut(kind.as_str()) {
                let taken = (*units).min(free);
                *units -= taken;
                free -= taken;
            }
        }
    }
    surplus
        .values()
        .fold(0_u32, |total, units| total.saturating_add(*units))
        .saturating_sub(room)
}

/// The first building of the wanted kind the planner may commission.
fn civic_def(world: &World, wanted: impl Fn(&BuildingDef) -> bool) -> Option<&BuildingDef> {
    economy_spec::registry(world)?
        .spec
        .buildings
        .iter()
        .find(|def| def.civic && wanted(def))
}

/// Whether the settlement could make the goods of `role` at a new workplace.
fn makeable(world: &World, settlement: SimId, role: Role) -> bool {
    if !catalog::recipes_for(world, role).is_empty()
        || role == super::super::roles::roles(world).hunter
    {
        return true;
    }
    let outputs = catalog::outputs_for(world, role);
    world.resource::<SimIndex>().map.values().any(|entity| {
        settlements::member(world, *entity) == Some(settlement)
            && world
                .get::<ResourcePatch>(*entity)
                .is_some_and(|patch| patch.remaining > 0 && outputs.contains(&patch.kind.as_str()))
    })
}

/// Seats taken and seats offered for `role` in the settlement.
fn seats(world: &World, settlement: SimId, role: Role) -> (u32, u32) {
    let mut taken = 0;
    let mut offered = 0;
    for (id, entity) in &world.resource::<SimIndex>().map {
        if settlements::member(world, *entity) == Some(settlement)
            && world
                .get::<VillagePlace>(*entity)
                .is_some_and(|place| place.role == role)
            && let Some(capacity) = labor::capacity(world, *id)
        {
            offered += capacity;
            taken += labor::assigned(world, *id);
        }
    }
    (taken, offered)
}

fn estimate(world: &World, def: &BuildingDef) -> u32 {
    def.cost
        .iter()
        .map(|stack| {
            stack
                .count
                .saturating_mul(market::unit_price(world, &stack.good).unwrap_or(0))
        })
        .fold(0, u32::saturating_add)
}

/// The nearest cell and turn around the settlement centre where `def` may
/// stand, in a fixed order: by distance, then row, then column.
pub fn find_site(
    world: &World,
    def: &BuildingDef,
    settlement: SimId,
    radius: u32,
) -> Option<(i32, i32, u8)> {
    let centre = world
        .resource::<SimIndex>()
        .get(settlement)
        .and_then(|entity| world.get::<Settlement>(entity))?
        .center;
    find_site_around(world, def, settlement, centre, SQUARE_CELLS, radius)
}

/// [`find_site`] around any point: the nearest valid cell at least `inner`
/// and at most `radius` cells from `origin` that belongs to `settlement`.
pub fn find_site_around(
    world: &World,
    def: &BuildingDef,
    settlement: SimId,
    origin: SimPos,
    inner: i32,
    radius: u32,
) -> Option<(i32, i32, u8)> {
    let (cx, cz) = (origin.x.div_euclid(1000), origin.z.div_euclid(1000));
    let radius = radius as i32;
    let mut cells = Vec::new();
    for dz in -radius..=radius {
        for dx in -radius..=radius {
            let distance = dx * dx + dz * dz;
            if distance <= radius * radius && distance >= inner * inner {
                cells.push((distance, cz + dz, cx + dx));
            }
        }
    }
    cells.sort_unstable();
    let survey = Survey::new(world);
    for (_, z, x) in cells {
        // Prefer the body on the far side of the entrance, so doors face in.
        let (dx, dz) = (x - cx, z - cz);
        let first: u8 = if dz.abs() >= dx.abs() {
            if dz >= 0 { 0 } else { 2 }
        } else if dx >= 0 {
            1
        } else {
            3
        };
        for turn in [0, 1, 3, 2] {
            let rotation = (first + turn) % 4;
            if let Ok((plot, owner)) = survey.check_fast(world, def, x, z, rotation)
                && owner == settlement
                && survey.check_reachable(world, &plot).is_ok()
            {
                return Some((x, z, rotation));
            }
        }
    }
    None
}

/// Whether the planner of a settlement could commission a home now: the data
/// has one it may build and the fund can pay for its materials.
pub fn can_build_home(world: &World, settlement: SimId) -> bool {
    let _ = settlement;
    civic_def(world, |def| matches!(def.kind, BuildingKind::Home { .. })).is_some_and(|def| {
        estimate(world, def) <= sites::budget(world).saturating_add(sites::levy_capacity(world))
    })
}

/// Beds of the largest home the planner may commission.
pub fn largest_home_beds(world: &World) -> u32 {
    economy_spec::registry(world).map_or(0, |registry| {
        registry
            .spec
            .buildings
            .iter()
            .filter(|def| def.civic)
            .filter_map(|def| match def.kind {
                BuildingKind::Home { beds } => Some(beds),
                _ => None,
            })
            .max()
            .unwrap_or(0)
    })
}

/// The field crop a settlement is shortest of with every plot of it sown:
/// `(crop key, unmet units)`, the most units first, then by key. More land
/// of that crop, not more labour, answers it.
fn crop_shortage(world: &World, settlement: SimId) -> Option<(String, u32)> {
    let crops = economy_spec::registry(world)?.spec.crops.clone();
    crops
        .into_iter()
        .filter(|crop| crate::village::fields::crop_land_is_short(world, settlement, &crop.key))
        .map(|crop| {
            let short = labor::good_shortage(world, settlement, &crop.harvest.good);
            (crop.key, short)
        })
        .filter(|(_, short)| *short > 0)
        .max_by(|(ak, a), (bk, b)| a.cmp(b).then_with(|| bk.cmp(ak)))
}

/// A planner decision as one sentence for players: the building by its
/// display name, what was decided and why.
fn sentence(world: &World, decision: &str, detail: &serde_json::Value) -> String {
    let building = detail
        .get("building")
        .and_then(serde_json::Value::as_str)
        .and_then(|id| super::definition(world, id))
        .map_or_else(|| "A building".to_owned(), |def| def.display.clone());
    let reason = detail
        .get("reason")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    let mut text = match decision.strip_prefix("deferred: ") {
        Some(why) => format!("{building} is wanted but waits: {why}"),
        None => format!("{building}: {decision}"),
    };
    if !reason.is_empty() {
        text.push_str(" (");
        text.push_str(reason);
        text.push(')');
    }
    text
}

fn decide(world: &mut World, settlement: SimId, decision: &str, detail: serde_json::Value) {
    let text = sentence(world, decision, &detail);
    let changed = {
        let mut civic = world.resource_mut::<CivicState>();
        let planner = civic.planners.entry(settlement).or_default();
        let changed = planner.last_decision != decision;
        if changed {
            planner.last_decision = decision.into();
            planner.last_decision_text = text;
        }
        changed
    };
    if changed {
        let mut payload = detail;
        payload["settlement"] = json!(settlement);
        payload["decision"] = json!(decision);
        record_effect(world, None, "VillageCivicProject", &payload);
    }
}

fn review(world: &mut World, settlement: SimId, now: u64) {
    let Some(tuning) = economy_spec::tuning(world).cloned() else {
        return;
    };
    sites::expire(world, settlement, now);
    depot::expire(world, settlement, now);
    crate::village::demography::rehouse_families(world, settlement);
    let (homeless, free_beds) = house_homeless(world, settlement);
    let (crowded, _) = crate::village::demography::crowding(world, settlement);
    // The largest crowded family a home of the list can hold: building for a
    // family no home fits would help nobody.
    let most_beds = largest_home_beds(world);
    let largest_family = crate::village::demography::crowded_families(world, settlement)
        .into_iter()
        .find(|size| *size <= most_beds)
        .unwrap_or(0);
    let youths = crate::village::demography::waiting_youths(world, settlement);
    let members = census(world, settlement).members;
    let surplus = unstored_surplus(world, settlement, &members);
    let projects = {
        let mut civic = world.resource_mut::<CivicState>();
        let planner = civic.planners.entry(settlement).or_default();
        planner.last_review = now;
        planner.homeless = homeless;
        planner.free_beds = free_beds;
        planner.surplus_units = surplus;
        planner.crowded = crowded;
        planner.pending_cost = 0;
        planner.projects.clone()
    };
    // The open projects: one not finished in time is cancelled, one that is
    // gone is forgotten. What each answers (`need_key`) is not answered twice.
    let stall = sites::stall_ticks(world);
    let mut open_keys: BTreeSet<String> = BTreeSet::new();
    let mut open = Vec::new();
    let mut in_progress = 0_usize;
    for site_id in projects {
        let Some(site) = world
            .resource::<SimIndex>()
            .get(site_id)
            .and_then(|entity| world.get::<ConstructionSite>(entity))
        else {
            continue;
        };
        if now.saturating_sub(site.placed_at) > tuning.planner_project_timeout_ticks {
            sites::cancel(world, site_id, "not finished in time");
            continue;
        }
        // A held-up project takes no builder or hauler: it does not count
        // against what the settlement can staff.
        if !sites::stalled(site, now, stall) {
            in_progress += 1;
        }
        if let Some(def) = super::definition(world, &site.building) {
            open_keys.insert(need_key(def));
        }
        open.push(site_id);
    }
    world
        .resource_mut::<CivicState>()
        .planners
        .entry(settlement)
        .or_default()
        .projects = open.clone();
    // A shortage must last before it is worth a building.
    let workplaces: Vec<(Role, String)> = economy_spec::trades(world)
        .into_iter()
        .filter_map(|role| {
            civic_def(world, |def| {
                matches!(&def.kind, BuildingKind::Workplace { profession, .. } if *profession == role)
            })
            .map(|def| (role, def.id.clone()))
        })
        .collect();
    let mut short_roles = Vec::new();
    for (role, _) in &workplaces {
        let (taken, offered) = seats(world, settlement, *role);
        // Every seat taken; or a trade of the catalog with no workplace here
        // at all (a trade added in data), whose goods are wanted.
        if (offered == 0 || taken >= offered)
            && labor::shortage(world, settlement, *role) >= tuning.planner_demand_units
            && makeable(world, settlement, *role)
        {
            short_roles.push(*role);
        }
    }
    let sustain = tuning.planner_sustain_reviews;
    // More land, not more labour, answers a crop shortage with every plot sown.
    let short_crop = crop_shortage(world, settlement)
        .filter(|(_, units)| *units >= tuning.planner_crop_shortage_units)
        .filter(|_| crate::village::calendar(world).1 != crate::village::Season::Winter);
    let crops_short = short_crop.as_ref().map_or(0, |(_, units)| *units);
    let land_short = short_crop.is_some();
    // A grown child without a seat needs a workplace: the trade whose goods
    // are shortest, else a farm.
    let youth_role = workplaces
        .iter()
        .map(|(role, _)| {
            (
                labor::shortage(world, settlement, *role),
                std::cmp::Reverse(*role),
            )
        })
        .filter(|(_, role)| {
            makeable(world, settlement, role.0) && !economy_spec::is_service(world, role.0)
        })
        .max()
        .map(|(_, role)| role.0);
    // The overseer's priorities (`oversee`): each civic need waits its own
    // number of reviews; all normal, every one waits `sustain`.
    use crate::village::frontier::oversee::{self, PriorityTopic};
    let level =
        |topic: PriorityTopic, role: Option<Role>| oversee::level(world, settlement, topic, role);
    let housing_level = level(PriorityTopic::Housing, None);
    let storage_level = level(PriorityTopic::Storage, None);
    let field_level = level(PriorityTopic::FoodReserve, None);
    let seat_level = level(PriorityTopic::Workplace, youth_role);
    let role_levels: std::collections::BTreeMap<Role, oversee::PriorityLevel> = workplaces
        .iter()
        .map(|(role, _)| (*role, level(PriorityTopic::Workplace, Some(*role))))
        .collect();
    // A need with an open project waits for it: its count starts again when
    // the project is finished or cancelled, as with one project at a time.
    let storage_id = civic_def(world, |def| {
        matches!(def.kind, BuildingKind::Storage { .. }) && def.field.is_none()
    })
    .map(need_key);
    let field_id = {
        let crop = short_crop.as_ref().map(|(key, _)| key.as_str());
        civic_def(world, |def| {
            def.field
                .as_ref()
                .is_some_and(|field| crop.is_none_or(|crop| field.crop == crop))
        })
        .map(need_key)
    };
    let answered = |key: &Option<String>| key.as_ref().is_some_and(|key| open_keys.contains(key));
    let home_open = open_keys.contains(HOME_KEY);
    let storage_open = answered(&storage_id);
    let field_open = answered(&field_id);
    let seat_open = youth_role
        .and_then(|role| workplaces.iter().find(|(candidate, _)| *candidate == role))
        .is_some_and(|(_, id)| open_keys.contains(id));
    let short_roles: Vec<Role> = short_roles
        .into_iter()
        .filter(|role| {
            !workplaces
                .iter()
                .any(|(candidate, id)| candidate == role && open_keys.contains(id))
        })
        .collect();
    let (beds_due, storage_due, role_due, field_due, seat_due) = {
        let mut civic = world.resource_mut::<CivicState>();
        let planner = civic.planners.entry(settlement).or_default();
        planner.bed_shortage_reviews = if (homeless > 0 || largest_family > 0) && !home_open {
            planner.bed_shortage_reviews.saturating_add(1)
        } else {
            0
        };
        planner.field_reviews = if land_short && !field_open {
            planner.field_reviews.saturating_add(1)
        } else {
            0
        };
        planner.seat_reviews = if youths > 0 && !seat_open {
            planner.seat_reviews.saturating_add(1)
        } else {
            0
        };
        planner.overflow_reviews =
            if surplus >= tuning.planner_storage_overflow_units && !storage_open {
                planner.overflow_reviews.saturating_add(1)
            } else {
                0
            };
        planner
            .demand_reviews
            .retain(|role, _| short_roles.contains(role));
        for role in &short_roles {
            let reviews = planner.demand_reviews.entry(*role).or_default();
            *reviews = reviews.saturating_add(1);
        }
        let role_due = planner
            .demand_reviews
            .iter()
            .filter(|(role, reviews)| {
                **reviews
                    >= oversee::sustain(
                        role_levels.get(*role).copied().unwrap_or_default(),
                        sustain,
                    )
            })
            .max_by_key(|(role, reviews)| (**reviews, std::cmp::Reverse(**role)))
            .map(|(role, _)| *role);
        (
            planner.bed_shortage_reviews >= oversee::sustain(housing_level, sustain),
            planner.overflow_reviews >= oversee::sustain(storage_level, sustain),
            role_due,
            planner.field_reviews >= oversee::sustain(field_level, sustain),
            planner.seat_reviews >= oversee::sustain(seat_level, sustain),
        )
    };
    // The planner's order (homes, fields, stores, seats, trades), with higher
    // priorities first; only the first due need is answered this review.
    let mut due = Vec::new();
    if beds_due {
        due.push((housing_level, 0_u8));
    }
    if field_due {
        due.push((field_level, 1));
    }
    if storage_due {
        due.push((storage_level, 2));
    }
    if seat_due {
        due.push((seat_level, 3));
    }
    if let Some(role) = role_due {
        due.push((role_levels.get(&role).copied().unwrap_or_default(), 4));
    }
    // The planner's choice for one due need: the building and its reason.
    let choose = |world: &World, need: u8| -> Option<(String, String)> {
        if need == 0 {
            // A crowded family needs a home that holds all of it; a single
            // resident without a bed needs the first home of the list.
            let home = |def: &BuildingDef, beds: u32| matches!(def.kind, BuildingKind::Home { beds: b } if b >= beds);
            let wanted = if homeless > 0 { 1 } else { largest_family };
            civic_def(world, |def| home(def, wanted)).map(|def| {
                (
                    def.id.clone(),
                    if homeless == 0 {
                        format!(
                            "{crowded} person(s) have slept in an overfull home for {sustain} \
                             reviews; the largest family counts {largest_family}"
                        )
                    } else {
                        format!("{homeless} resident(s) have had no bed for {sustain} reviews")
                    },
                )
            })
        } else if need == 1 {
            let crop = short_crop.as_ref().map(|(key, _)| key.as_str());
            civic_def(world, |def| {
                def.field
                    .as_ref()
                    .is_some_and(|field| crop.is_none_or(|crop| field.crop == crop))
            })
            .map(|def| {
                (
                    def.id.clone(),
                    format!(
                        "{crops_short} unit(s) of field crops are wanted and every plot is sown"
                    ),
                )
            })
        } else if need == 2 {
            civic_def(world, |def| {
                matches!(def.kind, BuildingKind::Storage { .. }) && def.field.is_none()
            })
            .map(|def| {
                (
                    def.id.clone(),
                    format!("{surplus} surplus unit(s) have had no store for {sustain} reviews"),
                )
            })
        } else if need == 3 {
            youth_role.and_then(|role| {
                workplaces
                .iter()
                .find(|(candidate, _)| *candidate == role)
                .map(|(_, id)| {
                    (
                        id.clone(),
                        format!(
                            "{youths} grown child(ren) have had no workplace seat for {sustain} \
                             reviews"
                        ),
                    )
                })
            })
        } else if need == 4 {
            role_due.and_then(|role| {
                workplaces
                    .iter()
                    .find(|(candidate, _)| *candidate == role)
                    .map(|(_, id)| {
                        (
                            id.clone(),
                            format!(
                                "{} unit(s) of {} goods are wanted and every seat is taken",
                                labor::shortage(world, settlement, role),
                                role.name()
                            ),
                        )
                    })
            })
        } else {
            None
        }
    };
    let slots = (members.len() / tuning.planner_residents_per_project.max(1) as usize).max(1);
    let mut deferral: Option<(String, serde_json::Value)> = None;
    for need in oversee::order_projects(due) {
        let Some((building, reason)) = choose(world, need) else {
            continue;
        };
        let Some(def) = economy_spec::registry(world)
            .and_then(|registry| registry.spec.building(&building))
            .cloned()
        else {
            continue;
        };
        if open_keys.contains(&need_key(&def)) {
            continue;
        }
        match commission(world, settlement, &def, &reason, &open, in_progress, slots) {
            Commission::Placed => return,
            // Coins and builders bind every need alike: a lower need does
            // not jump ahead of one that waits for them.
            Commission::Wait(decision, detail) => {
                decide(world, settlement, &decision, detail);
                return;
            }
            Commission::Skip(decision, detail) => {
                deferral.get_or_insert((decision, detail));
            }
        }
    }
    if let Some((decision, detail)) = deferral {
        decide(world, settlement, &decision, detail);
    }
}

/// What the planner keys an open project by: one project answers each need.
/// Every home answers the one need for beds; any other building its own.
const HOME_KEY: &str = "home";
fn need_key(def: &BuildingDef) -> String {
    if matches!(def.kind, BuildingKind::Home { .. }) {
        HOME_KEY.to_owned()
    } else {
        def.id.clone()
    }
}

/// The outcome of one attempt to open a civic project.
enum Commission {
    /// The site is open.
    Placed,
    /// Nothing more opens this review: the fund or the builders are short.
    Wait(String, serde_json::Value),
    /// This building cannot open now; a lower need may.
    Skip(String, serde_json::Value),
}

/// Opens the site of `def` for the planner of `settlement`, if the
/// settlement can staff and supply it. The first project of a settlement
/// needs only the coins and a spot, as when one project was the rule; a
/// further one needs, besides, a free builder (fewer projects that can
/// progress than `slots`), a place in the open-site allowance that leaves
/// one for the overseer, no good an open home still lacks, materials that
/// can come (`supply::materials_reachable`) and coins for itself on top of
/// what the settlement's open sites still need.
fn commission(
    world: &mut World,
    settlement: SimId,
    def: &BuildingDef,
    reason: &str,
    open: &[SimId],
    in_progress: usize,
    slots: usize,
) -> Commission {
    let Some(tuning) = economy_spec::tuning(world).cloned() else {
        return Commission::Wait("deferred: no economy".into(), json!({}));
    };
    let building = def.id.clone();
    let cost = estimate(world, def);
    let fund = world.resource::<VillageState>().fund;
    let further = !open.is_empty();
    if further {
        if in_progress >= slots {
            return Commission::Wait(
                "deferred: every builder the settlement can spare is busy".into(),
                json!({"building":building,"reason":reason,"projects":open.len(),
                    "in_progress":in_progress,"slots":slots}),
            );
        }
        if sites::open_building_sites(world, settlement) + 1 >= super::max_open_sites(world) {
            return Commission::Wait(
                "deferred: the last open site is kept for the overseer".into(),
                json!({"building":building,"reason":reason,"projects":open.len()}),
            );
        }
        let goods: super::model::Goods = def
            .cost
            .iter()
            .map(|stack| (stack.good.clone(), stack.count))
            .collect();
        // A home outranks another project for the goods it still lacks
        // (`sites::home_waits_for`): a further project needing them waits
        // until the home has them.
        if let Some(kind) = goods
            .keys()
            .find(|kind| sites::home_waits_for(world, settlement, kind))
        {
            return Commission::Skip(
                format!("deferred: a home still waits for {kind}"),
                json!({"building":building,"reason":reason,"good":kind}),
            );
        }
        if let Err(kind) = super::supply::materials_reachable(world, settlement, &goods) {
            return Commission::Skip(
                format!("deferred: no {kind} can reach another site now"),
                json!({"building":building,"reason":reason,"good":kind}),
            );
        }
    }
    // What the fund may spend now, plus what the building levy can raise;
    // a further project also leaves what the open sites still need.
    let committed = if further {
        sites::outstanding_cost_of(world, settlement)
    } else {
        0
    };
    if cost.saturating_add(committed)
        > sites::budget(world).saturating_add(sites::levy_capacity(world))
    {
        world
            .resource_mut::<CivicState>()
            .planners
            .entry(settlement)
            .or_default()
            .pending_cost = cost;
        return Commission::Wait(
            "deferred: the civic fund cannot pay for the materials yet".into(),
            json!({"building":building,"reason":reason,"estimated_cost":cost,"fund":fund,
                "committed":committed}),
        );
    }
    let Some((x, z, rotation)) =
        find_site(world, def, settlement, tuning.planner_search_radius_cells)
    else {
        return Commission::Skip(
            "deferred: no buildable site near the centre".into(),
            json!({"building":building,"reason":reason}),
        );
    };
    match placement::place(world, &building, x, z, rotation, true, reason) {
        Ok(site) => {
            {
                let mut civic = world.resource_mut::<CivicState>();
                let planner = civic.planners.entry(settlement).or_default();
                planner.projects.push(site);
                planner.projects.sort_unstable();
                // The need this answers starts its count again; the others
                // keep theirs.
                match &def.kind {
                    BuildingKind::Home { .. } => planner.bed_shortage_reviews = 0,
                    BuildingKind::Workplace { profession, .. } => {
                        planner.demand_reviews.remove(profession);
                        planner.seat_reviews = 0;
                    }
                    _ if def.field.is_some() => planner.field_reviews = 0,
                    _ => planner.overflow_reviews = 0,
                }
                planner.last_decision = format!("commissioned {building}");
                planner.last_decision_text =
                    format!("Commissioned a {}: {reason}", def.display.to_lowercase());
            }
            record_effect(
                world,
                None,
                "VillageCivicProject",
                &json!({"settlement":settlement,"decision":"commissioned","building":building,
                    "site":site,"reason":reason,"cell":[x,z],"rotation":rotation,
                    "estimated_cost":cost,"fund":fund,"projects":open.len() + 1}),
            );
            Commission::Placed
        }
        Err(error) => Commission::Skip(
            "deferred: the chosen site was refused".into(),
            json!({"building":building,"reason":reason,"refusal":error}),
        ),
    }
}
