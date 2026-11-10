//! Regional residents and employment. The life tree alone chooses physical work.
//!
//! A reassignment changes a person's workplace at an idle Work boundary. It never
//! moves the person, replaces their identity, grants tools, or cancels a batch.
use super::{SettlementMember, routes, settlements};
use crate::village::{self, Role, Villager, Wallet, catalog, economy, production};
use bevy_app::App;
use bevy_ecs::prelude::*;
use bevy_reflect::Reflect;
use npc_framework::agent_io::control::ExternalController;
use npc_framework::agent_io::{AgentIoAppExt, Section};
use npc_framework::kernel::commands::CommandError;
use npc_framework::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::BTreeSet;

pub const ROLES: [Role; 13] = [
    Role::Farmer,
    Role::Miller,
    Role::Baker,
    Role::WaterKeeper,
    Role::Woodcutter,
    Role::Carpenter,
    Role::Miner,
    Role::Smith,
    Role::Innkeeper,
    Role::Hunter,
    Role::Tanner,
    Role::Tailor,
    Role::Fisher,
];
const REVIEW_TICKS: u64 = 300;
const BLOCKED_TICKS: u64 = 1200;
const SWITCH_COOLDOWN: u64 = 6000;

/// Stable character identity and tastes; none of these change with employment.
#[derive(Component, Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Reflect)]
#[reflect(Component)]
pub struct ResidentProfile {
    pub key: String,
    pub appearance: u8,
    pub preferred_food: String,
    pub allowed_roles: Vec<Role>,
    pub flexible: bool,
}
impl SimHash for ResidentProfile {
    const NAME: &'static str = "regional_resident_profile_v1";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.str(&self.key);
        h.u8(self.appearance);
        h.str(&self.preferred_food);
        h.u8(u8::from(self.flexible));
        h.u32(self.allowed_roles.len() as u32);
        for role in &self.allowed_roles {
            h.u8(role.tag());
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub struct AssignmentRequest {
    pub workplace: SimId,
    pub requested_at: u64,
}
#[derive(Component, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Reflect)]
#[reflect(Component)]
pub struct Employment {
    pub changed_at: u64,
    pub cooldown_until: u64,
    pub next_review: u64,
    pub blocked_since: Option<u64>,
    pub pending: Option<AssignmentRequest>,
    pub last_reason: String,
}
impl SimHash for Employment {
    const NAME: &'static str = "regional_employment_v1";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.u64(self.changed_at);
        h.u64(self.cooldown_until);
        h.u64(self.next_review);
        h.opt(self.blocked_since.as_ref(), |h, tick| h.u64(*tick));
        h.opt(self.pending.as_ref(), |h, request| {
            h.u32(request.workplace.0);
            h.u64(request.requested_at);
        });
        h.str(&self.last_reason);
    }
}

pub fn profile(world: &World, actor: Entity) -> Option<&ResidentProfile> {
    world.get::<ResidentProfile>(actor)
}

pub fn register(app: &mut App) {
    app.register_sim_component::<ResidentProfile>()
        .register_sim_component::<Employment>()
        .register_command::<AssignProfession>()
        .register_observation("regional_employment", |world, ctx| {
            let view = summary(world, ctx.entity)?;
            Some(Section::new(json!(view)).line(format!("Work: {}. {}", view.role, view.reason)))
        });
}

const NAMES: [[&str; 14]; 3] = [
    [
        "Ada", "Rowan", "Mira", "Nell", "Oak", "Jun", "Bram", "Ivo", "Tess", "Rhea", "Sage",
        "Lina", "Finn", "Piper",
    ],
    [
        "Fern", "Ellis", "Dara", "Wren", "Ash", "Moss", "Orin", "Ember", "Hazel", "Reed", "Aster",
        "Iris", "Marin", "Kit",
    ],
    [
        "Clover", "Alden", "Sora", "Brook", "Cedar", "Robin", "Flint", "Niko", "Mallow", "Kestrel",
        "Sorrel", "Faye", "Coral", "Remy",
    ],
];

fn aptitudes(role: Role, flexible: bool) -> Vec<Role> {
    let mut roles = if flexible {
        ROLES.to_vec()
    } else {
        match role {
            Role::Farmer | Role::WaterKeeper | Role::Woodcutter | Role::Fisher => vec![
                Role::Farmer,
                Role::WaterKeeper,
                Role::Woodcutter,
                Role::Fisher,
            ],
            Role::Miller | Role::Baker => vec![Role::Miller, Role::Baker, Role::Farmer],
            Role::Carpenter => vec![Role::Carpenter, Role::Woodcutter, Role::Miller],
            Role::Miner | Role::Smith => vec![Role::Miner, Role::Smith, Role::Woodcutter],
            Role::Hunter | Role::Tanner | Role::Tailor => {
                vec![Role::Hunter, Role::Tanner, Role::Tailor, Role::Farmer]
            }
            Role::Innkeeper => vec![
                Role::Innkeeper,
                Role::Baker,
                Role::Miller,
                Role::WaterKeeper,
            ],
            Role::Visitor => Vec::new(),
        }
    };
    roles.sort_unstable();
    roles.dedup();
    roles
}

/// One finite initial endowment, used only during construction of a new world.
pub fn spawn(world: &mut World) {
    let tree = world
        .resource::<AiRegistry>()
        .tree_id(village::behavior::TREE)
        .expect("regional life tree");
    for (region_index, (settlement, _)) in settlements::all(world).into_iter().enumerate() {
        let definition = settlements::definition(world, settlement)
            .expect("settlement definition")
            .clone();
        for (index, base_name) in NAMES[region_index % NAMES.len()]
            .iter()
            .copied()
            .enumerate()
        {
            let role = ROLES.get(index).copied().unwrap_or(Role::Woodcutter);
            let home = &definition.homes[index];
            let home_entity =
                settlements::home_entity(world, settlement, &home.key).expect("resident home");
            let workplace =
                settlements::workplace(world, settlement, role).expect("resident workplace");
            let flexible = index == 13 || role == Role::Innkeeper;
            let template = match role {
                Role::Carpenter => "carpenter",
                Role::Smith | Role::Miner | Role::Innkeeper => "smith",
                Role::Woodcutter if flexible => "carpenter",
                Role::Woodcutter => "woodcutter",
                _ => "novice",
            };
            let spec = crate::layout::UnitSpec {
                template: template.into(),
                behaviour: crate::Behaviour::Idle,
                ..Default::default()
            };
            let id = crate::layout::spawn_unit(world, home.door, &spec)
                .expect("regional resident template");
            let actor = world
                .resource::<SimIndex>()
                .get(id)
                .expect("resident identity");
            let name = if region_index < NAMES.len() {
                base_name.to_owned()
            } else {
                format!("{base_name} of {}", definition.label)
            };
            village::initialise_person(world, actor, role, &name, home.door, workplace);
            world
                .entity_mut(actor)
                .remove::<(BrainState, ActiveLeaf)>()
                .insert((
                    Brain { tree },
                    Inventory::new(64, 500_000),
                    SettlementMember { settlement },
                    ResidentProfile {
                        key: format!("{}.resident.{index}", definition.key),
                        appearance: (region_index * 14 + index) as u8,
                        preferred_food: if index.is_multiple_of(3) {
                            catalog::COOKED_MEAL
                        } else {
                            "bread"
                        }
                        .into(),
                        allowed_roles: aptitudes(role, flexible),
                        flexible,
                    },
                    Employment {
                        last_reason: "Established at the village worksite".into(),
                        ..Default::default()
                    },
                ));
            world
                .get_mut::<Wallet>(actor)
                .expect("resident wallet")
                .coins = village::NPC_SEED_COINS;
            world
                .get_mut::<village::colony::Household>(home_entity)
                .expect("household")
                .residents
                .push(id);
            for (kind, count) in [("bread", 2), (catalog::WATER, 2), ("log", 2)] {
                economy::grant(world, actor, Item::new(kind).stack(count));
            }
            for kind in catalog::outputs_for(world, role) {
                if !matches!(
                    *kind,
                    "bread" | "log" | catalog::WATER | catalog::WARM_CLOTHES
                ) {
                    if matches!(*kind, "hammer" | catalog::SPEAR | catalog::FISHING_ROD) {
                        for _ in 0..2 {
                            economy::grant(world, actor, Item::new(*kind));
                        }
                    } else {
                        economy::grant(world, actor, Item::new(*kind).stack(2));
                    }
                }
            }
            if role == Role::Hunter {
                for _ in 0..2 {
                    economy::grant(world, actor, Item::new(catalog::SPEAR));
                }
            }
            if role == Role::Tanner {
                economy::grant(world, actor, Item::new(catalog::BOWL));
            }
            if role == Role::Fisher {
                economy::grant(world, actor, Item::new(catalog::FISHING_ROD));
            }
            let mut ids = world.resource_mut::<crate::TestbedIds>();
            ids.name(name, id);
            ids.name(format!("{}.resident.{index}", definition.key), id);
            ids.units.push(id);
        }
    }
    let day = village::calendar(world).0;
    world
        .resource_mut::<village::VillageState>()
        .last_dividend_day = day;
}

fn site(world: &World, workplace: SimId) -> Option<(Entity, Role, u32)> {
    let entity = world.resource::<SimIndex>().get(workplace)?;
    let role = world.get::<village::VillagePlace>(entity)?.role;
    let settlement = settlements::member(world, entity)?;
    let definition = settlements::definition(world, settlement)?;
    let key = &world.get::<super::SettlementSite>(entity)?.key;
    let capacity = definition
        .sites
        .iter()
        .find(|site| &site.key == key)?
        .capacity;
    Some((entity, role, capacity))
}

pub fn assigned(world: &World, workplace: SimId) -> u32 {
    world
        .resource::<SimIndex>()
        .map
        .values()
        .filter(|entity| {
            village::living(world, **entity)
                && world.get::<Villager>(**entity).is_some_and(|person| {
                    person.workplace == workplace && person.role != Role::Visitor
                })
        })
        .count() as u32
}

fn qualified(world: &World, actor: Entity, role: Role) -> bool {
    if profile(world, actor).is_none_or(|profile| !profile.allowed_roles.contains(&role)) {
        return false;
    }
    let skill = match role {
        Role::Smith => Some("Smithing"),
        Role::Carpenter => Some("Carpentry"),
        _ => None,
    };
    skill.is_none_or(|skill| {
        world.get::<SimId>(actor).is_some_and(|id| {
            npc_framework::rules::effective_skill(world, *id, skill).is_some_and(|level| level >= 8)
        })
    })
}

fn legal_assignment(world: &World, actor: Entity, workplace: SimId) -> Result<Role, String> {
    if !village::living(world, actor) || profile(world, actor).is_none() {
        return Err("Choose a living regional resident".into());
    }
    let (target, role, capacity) = site(world, workplace).ok_or("Choose a regional workplace")?;
    if !qualified(world, actor, role) {
        return Err("This resident lacks the occupation's aptitude or skill".into());
    }
    let current = world
        .get::<Villager>(actor)
        .ok_or("Resident life is missing")?;
    if current.workplace != workplace && assigned(world, workplace) >= capacity {
        return Err("That workplace has no vacant position".into());
    }
    let here = *world
        .get::<SimPos>(actor)
        .ok_or("Resident position is missing")?;
    let there = *world
        .get::<SimPos>(target)
        .ok_or("Workplace position is missing")?;
    if routes::estimate(world, here, there).is_none() {
        return Err("No usable route to that workplace was found".into());
    }
    Ok(role)
}

/// Inspector requests reserve no materials and do not seize action ownership.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssignProfession {
    pub resident: SimId,
    pub workplace: SimId,
}
impl AssignProfession {
    pub fn check(&self, world: &World) -> Result<(), String> {
        let actor = world
            .resource::<SimIndex>()
            .get(self.resident)
            .ok_or("Resident is missing")?;
        legal_assignment(world, actor, self.workplace).map(|_| ())
    }
    pub fn perform(&self, world: &mut World) -> Result<(), String> {
        self.check(world)?;
        let actor = world
            .resource::<SimIndex>()
            .get(self.resident)
            .expect("checked resident");
        let now = world.resource::<SimClock>().tick;
        let mut employment = world
            .get_mut::<Employment>(actor)
            .ok_or("Employment is missing")?;
        employment.pending = Some(AssignmentRequest {
            workplace: self.workplace,
            requested_at: now,
        });
        record_effect(
            world,
            Some(self.resident),
            "RegionalAssignmentRequested",
            &json!({"workplace":self.workplace}),
        );
        Ok(())
    }
}
impl ContentCommand for AssignProfession {
    const NAME: &'static str = "AssignProfession";
    fn validate(&self, world: &World) -> Result<(), CommandError> {
        self.check(world)
            .map_err(|reason| CommandError::Unavailable { reason })
    }
    fn apply(self, world: &mut World) {
        let _ = self.perform(world);
    }
    fn unit(&self) -> Option<SimId> {
        Some(self.resident)
    }
}

fn safe_boundary(world: &World, actor: Entity) -> bool {
    village::living(world, actor)
        && world.get::<ExternalController>(actor).is_none()
        && world
            .get::<OrderQueue>(actor)
            .is_none_or(OrderQueue::is_empty)
        && matches!(village::activity(world, actor).0, "idle" | "walking")
        && ["Hunger", "Thirst", "Energy", "Warmth"]
            .iter()
            .all(|need| economy::need(world, actor, need) >= 20_000)
        && world
            .get_resource::<village::colony::ColonyState>()
            .is_none_or(|state| {
                !state
                    .projects
                    .values()
                    .any(|project| Some(&project.builder) == world.get::<SimId>(actor))
            })
}

fn essential(role: Role) -> bool {
    matches!(
        role,
        Role::Farmer | Role::Miller | Role::Baker | Role::WaterKeeper | Role::Woodcutter
    )
}

/// Physical sources and tools, never a cash or material grant. Manufacturing may
/// wait for its upstream chain; an empty pack by itself is not job obsolescence.
fn viable(world: &World, actor: Entity, role: Role, workplace: SimId) -> bool {
    let Some((station, _, _)) = site(world, workplace) else {
        return false;
    };
    let Some(here) = world.get::<SimPos>(actor).copied() else {
        return false;
    };
    if world
        .get::<SimPos>(station)
        .is_none_or(|pos| routes::estimate(world, here, *pos).is_none())
    {
        return false;
    }
    if role == Role::Hunter {
        return tool_obtainable(world, actor, catalog::SPEAR)
            && village::hunting::available_prey(world)
                .into_iter()
                .any(|(_, _, position)| routes::estimate(world, here, position).is_some());
    }
    if role == Role::Fisher && !tool_obtainable(world, actor, catalog::FISHING_ROD) {
        return false;
    }
    if !catalog::recipes_for(world, role).is_empty() || role == Role::Innkeeper {
        return true;
    }
    catalog::outputs_for(world, role).iter().any(|kind| {
        world.resource::<SimIndex>().map.values().any(|entity| {
            world
                .get::<village::ResourcePatch>(*entity)
                .is_some_and(|patch| {
                    patch.kind == *kind
                        && patch.remaining > 0
                        && world
                            .get::<SimPos>(*entity)
                            .is_some_and(|pos| routes::estimate(world, here, *pos).is_some())
                })
        })
    })
}

fn tool_obtainable(world: &World, actor: Entity, kind: &str) -> bool {
    if economy::count(world, actor, kind) > 0 {
        return true;
    }
    let price = village::market::unit_price(world, kind).unwrap_or(u32::MAX);
    if world
        .get::<Wallet>(actor)
        .is_none_or(|wallet| wallet.coins < price)
    {
        return false;
    }
    let settlement = production::economic_settlement(world, actor);
    world.resource::<SimIndex>().map.values().any(|maker| {
        village::living(world, *maker)
            && production::economic_settlement(world, *maker) == settlement
            && world
                .get::<Villager>(*maker)
                .is_some_and(|person| catalog::outputs_for(world, person.role).contains(&kind))
    })
}

pub fn work_feasible(world: &World, actor: Entity) -> bool {
    world
        .get::<Villager>(actor)
        .is_some_and(|person| viable(world, actor, person.role, person.workplace))
}

/// Saved blockage timing is maintained by the existing island Effects pass.
/// This does not choose a goal or initiate any physical action.
pub fn tick(world: &mut World) {
    let now = world.resource::<SimClock>().tick;
    if !now.is_multiple_of(REVIEW_TICKS) {
        return;
    }
    let people: Vec<_> = world
        .resource::<SimIndex>()
        .map
        .values()
        .copied()
        .filter(|actor| profile(world, *actor).is_some())
        .collect();
    for actor in people {
        let alive = village::living(world, actor);
        let viable = world
            .get::<Villager>(actor)
            .is_some_and(|person| viable(world, actor, person.role, person.workplace));
        let mut employment = world
            .get_mut::<Employment>(actor)
            .expect("resident employment");
        if !alive {
            employment.pending = None;
            employment.blocked_since = None;
        } else if viable {
            employment.blocked_since = None;
        } else if employment.blocked_since.is_none() {
            employment.blocked_since = Some(now);
        }
    }
}

/// Public unmet demand aggregated from this settlement's finite funded plan.
pub fn shortage(world: &World, settlement: SimId, role: Role) -> u32 {
    let Some(plan) = world.get_resource::<production::ProductionPlan>() else {
        return 0;
    };
    let outputs = catalog::outputs_for(world, role);
    let mut wanted = 0_u32;
    let mut stock = 0_u32;
    for (id, actor) in &world.resource::<SimIndex>().map {
        if production::economic_settlement(world, *actor) != Some(settlement)
            || !village::living(world, *actor)
        {
            continue;
        }
        for kind in outputs {
            wanted = wanted.saturating_add(
                plan.procurement
                    .get(id)
                    .and_then(|goods| goods.get(*kind))
                    .copied()
                    .unwrap_or(0),
            );
            stock = stock.saturating_add(economy::count(world, *actor, kind));
        }
    }
    wanted.saturating_sub(stock)
}

#[derive(Clone, Debug, Serialize)]
pub struct Vacancy {
    pub workplace: SimId,
    pub settlement: SimId,
    pub role: String,
    pub occupied: u32,
    pub capacity: u32,
    pub shortage: u32,
}
pub fn vacancies(world: &World) -> Vec<Vacancy> {
    world
        .resource::<SimIndex>()
        .map
        .iter()
        .filter_map(|(id, entity)| {
            let (_, role, capacity) = site(world, *id)?;
            let settlement = settlements::member(world, *entity)?;
            let occupied = assigned(world, *id);
            (occupied < capacity).then(|| Vacancy {
                workplace: *id,
                settlement,
                role: role.name().into(),
                occupied,
                capacity,
                shortage: shortage(world, settlement, role),
            })
        })
        .collect()
}

/// Called only by the resident Work executor after immediate subsistence.
/// Returns true when the assignment changed; the next decision receives a new
/// funded production plan, rather than executing yesterday's job after a switch.
pub fn consider_assignment(world: &mut World, actor: Entity) -> bool {
    let Some(employment) = world.get::<Employment>(actor).cloned() else {
        return false;
    };
    if !safe_boundary(world, actor) {
        return false;
    }
    let now = world.resource::<SimClock>().tick;
    let person = world.get::<Villager>(actor).expect("resident").clone();
    if let Some(request) = employment.pending {
        match legal_assignment(world, actor, request.workplace) {
            Ok(role) => {
                return change(
                    world,
                    actor,
                    role,
                    request.workplace,
                    "Requested workplace assignment",
                );
            }
            Err(reason) => {
                let mut employment = world.get_mut::<Employment>(actor).expect("employment");
                employment.pending = None;
                employment.last_reason = format!("Assignment unavailable: {reason}");
                return false;
            }
        }
    }
    if now < employment.cooldown_until || now < employment.next_review || now < REVIEW_TICKS {
        return false;
    }
    world
        .get_mut::<Employment>(actor)
        .expect("employment")
        .next_review = now.saturating_add(REVIEW_TICKS);
    let blocked = employment
        .blocked_since
        .is_some_and(|since| now >= since.saturating_add(BLOCKED_TICKS));
    let duplicate = assigned(world, person.workplace) > 1;
    if essential(person.role) && !duplicate && !blocked {
        return false;
    }
    if !blocked && !duplicate && profile(world, actor).is_none_or(|profile| !profile.flexible) {
        return false;
    }
    let settlement = settlements::member(world, actor);
    let Some(here) = world.get::<SimPos>(actor).copied() else {
        return false;
    };
    let mut candidates = Vec::new();
    for vacancy in vacancies(world) {
        if vacancy.workplace == person.workplace {
            continue;
        }
        let Ok(role) = legal_assignment(world, actor, vacancy.workplace) else {
            continue;
        };
        if role == Role::Innkeeper {
            continue;
        }
        if !viable(world, actor, role, vacancy.workplace) {
            continue;
        }
        // A vacancy can be valuable before the missing profession has produced
        // any profit. Otherwise require a genuine shortage, not empty capacity.
        if vacancy.occupied > 0 && vacancy.shortage == 0 && !blocked {
            continue;
        }
        if !blocked && !duplicate && person.role != Role::Innkeeper && vacancy.occupied > 0 {
            continue;
        }
        let target = world
            .resource::<SimIndex>()
            .get(vacancy.workplace)
            .expect("workplace");
        let Some(route) = routes::estimate(
            world,
            here,
            *world.get::<SimPos>(target).expect("site position"),
        ) else {
            continue;
        };
        let local = settlement == Some(vacancy.settlement);
        let urgency = if vacancy.occupied == 0 && essential(role) {
            100_000_u64
        } else if vacancy.occupied == 0 {
            50_000
        } else {
            10_000
        };
        let score = urgency
            .saturating_add(u64::from(vacancy.shortage.min(50)) * 100)
            .saturating_add(if local { 5000 } else { 0 })
            .saturating_sub(u64::from(route.distance_mm / 20));
        // Hysteresis also protects useful nonessential specialist work.
        if score > 15_000 || blocked {
            candidates.push((score, vacancy.workplace, role));
        }
    }
    candidates.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    let Some((_, workplace, role)) = candidates.into_iter().next() else {
        return false;
    };
    change(
        world,
        actor,
        role,
        workplace,
        if blocked {
            "Resources or access unavailable; taking useful work"
        } else {
            "Filling an undersupplied village occupation"
        },
    )
}

fn change(world: &mut World, actor: Entity, role: Role, workplace: SimId, reason: &str) -> bool {
    let now = world.resource::<SimClock>().tick;
    let previous = world.get::<Villager>(actor).expect("resident").clone();
    if previous.role == role && previous.workplace == workplace {
        world
            .get_mut::<Employment>(actor)
            .expect("employment")
            .pending = None;
        return false;
    }
    {
        let mut person = world.get_mut::<Villager>(actor).expect("resident");
        person.role = role;
        person.workplace = workplace;
    }
    world.entity_mut(actor).remove::<MoveIntent>();
    production::clear_intent(world, actor);
    *world.get_mut::<Employment>(actor).expect("employment") = Employment {
        changed_at: now,
        cooldown_until: now.saturating_add(SWITCH_COOLDOWN),
        next_review: now.saturating_add(REVIEW_TICKS),
        blocked_since: None,
        pending: None,
        last_reason: reason.into(),
    };
    production::invalidate(world);
    let id = *world.get::<SimId>(actor).expect("resident identity");
    record_effect(
        world,
        Some(id),
        "RegionalOccupationChanged",
        &json!({
            "from":previous.role.name(), "to":role.name(), "from_workplace":previous.workplace,
            "workplace":workplace, "reason":reason,
        }),
    );
    true
}

#[derive(Clone, Debug, Serialize)]
pub struct LaborSummary {
    pub resident: SimId,
    pub key: String,
    pub settlement: SimId,
    pub role: String,
    pub workplace: SimId,
    pub reason: String,
    pub pending_workplace: Option<SimId>,
    pub cooldown_remaining: u64,
    pub allowed_roles: Vec<String>,
}
pub fn summary(world: &World, actor: Entity) -> Option<LaborSummary> {
    let identity = profile(world, actor)?;
    let person = world.get::<Villager>(actor)?;
    let employment = world.get::<Employment>(actor)?;
    Some(LaborSummary {
        resident: *world.get::<SimId>(actor)?,
        key: identity.key.clone(),
        settlement: settlements::member(world, actor)?,
        role: person.role.name().into(),
        workplace: person.workplace,
        reason: employment.last_reason.clone(),
        pending_workplace: employment.pending.as_ref().map(|pending| pending.workplace),
        cooldown_remaining: employment
            .cooldown_until
            .saturating_sub(world.resource::<SimClock>().tick),
        allowed_roles: identity
            .allowed_roles
            .iter()
            .filter(|role| qualified(world, actor, **role))
            .map(|role| role.name().into())
            .collect(),
    })
}

pub fn validate(world: &World) -> Result<(), String> {
    let mut keys = BTreeSet::new();
    for (id, actor) in &world.resource::<SimIndex>().map {
        let Some(identity) = profile(world, *actor) else {
            continue;
        };
        let person = world
            .get::<Villager>(*actor)
            .ok_or("Regional identity has no resident life")?;
        if identity.key.is_empty()
            || !keys.insert(identity.key.as_str())
            || identity.allowed_roles.is_empty()
            || identity
                .allowed_roles
                .windows(2)
                .any(|pair| pair[0] >= pair[1])
            || !identity.allowed_roles.contains(&person.role)
            || world.get::<Employment>(*actor).is_none()
            || settlements::member(world, *actor).is_none()
            || site(world, person.workplace).is_none_or(|(_, role, _)| role != person.role)
        {
            return Err(format!(
                "Regional resident {} has inconsistent identity or employment",
                id.0
            ));
        }
        let employment = world.get::<Employment>(*actor).expect("checked employment");
        let now = world.resource::<SimClock>().tick;
        if employment.changed_at > now
            || employment.blocked_since.is_some_and(|tick| tick > now)
            || employment.last_reason.len() > 1024
            || employment.pending.as_ref().is_some_and(|pending| {
                pending.requested_at > now
                    || site(world, pending.workplace)
                        .is_none_or(|(_, role, _)| !identity.allowed_roles.contains(&role))
            })
        {
            return Err(format!(
                "Regional resident {} has invalid employment history",
                id.0
            ));
        }
    }
    if keys.len() != settlements::all(world).len() * 14 {
        return Err("Regional founding identities are incomplete".into());
    }
    for id in world.resource::<SimIndex>().map.keys() {
        if let Some((_, _, capacity)) = site(world, *id)
            && assigned(world, *id) > capacity
        {
            return Err("Regional workplace is over capacity".into());
        }
    }
    Ok(())
}
