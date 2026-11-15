//! Regional residents and employment. The life tree alone chooses physical work.
//!
//! A reassignment changes a person's workplace at an idle Work boundary. It never
//! moves the person, replaces their identity, grants tools, or cancels a batch.
use super::{SettlementMember, economy_spec, routes, settlements};
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

pub use super::economy_spec::ROLES;
const REVIEW_TICKS: u64 = 300;
const BLOCKED_TICKS: u64 = 1200;
const SWITCH_COOLDOWN: u64 = 6000;
// The three margin constants are the built-in defaults of `economy.tuning`.
/// A vacancy must out-earn the current occupation by this much (permille of the
/// good's seed price) before it counts as a margin gap.
pub const MARGIN_GAP_PERMILLE: i64 = 250;
/// The gap must persist this long before it may move a resident.
pub const MARGIN_PERSIST_TICKS: u64 = 2400;
/// Switching cooldown once a margin gap has persisted (instead of 6000).
pub const MARGIN_COOLDOWN: u64 = 2400;

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
            role.sim_hash(h);
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub struct AssignmentRequest {
    pub workplace: SimId,
    pub requested_at: u64,
}
/// A profession a resident left, kept while it still limits a return.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub struct Departure {
    pub role: Role,
    pub workplace: SimId,
    pub tick: u64,
}
/// Departures remembered per resident.
pub const MAX_DEPARTURES: usize = 8;
/// A resident does not return to a profession left less than this long ago,
/// unless an essential workplace stands empty. Default of
/// `economy.tuning.profession_return_ticks` (two days).
pub const PROFESSION_RETURN_TICKS: u64 = 48_000;
/// A seat somebody just left is not refilled for this long, so two residents
/// cannot swap jobs in one reallocation pass. Default of
/// `economy.tuning.vacated_seat_ticks`.
pub const VACATED_SEAT_TICKS: u64 = 2_400;

#[derive(Component, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Reflect)]
#[reflect(Component)]
pub struct Employment {
    pub changed_at: u64,
    pub cooldown_until: u64,
    pub next_review: u64,
    pub blocked_since: Option<u64>,
    pub pending: Option<AssignmentRequest>,
    pub last_reason: String,
    /// First review at which a legal local vacancy out-earned this job by
    /// [`MARGIN_GAP_PERMILLE`]; cleared as soon as the gap closes.
    #[serde(default)]
    pub margin_gap_since: Option<u64>,
    /// Professions left recently, oldest first; see [`PROFESSION_RETURN_TICKS`].
    #[serde(default)]
    pub departures: Vec<Departure>,
    /// A service keeper's takings on `takings_day` (guests' fees).
    #[serde(default)]
    pub takings: u32,
    #[serde(default)]
    pub takings_day: u64,
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
        h.opt(self.margin_gap_since.as_ref(), |h, tick| h.u64(*tick));
        h.u32(self.departures.len() as u32);
        for departure in &self.departures {
            departure.role.sim_hash(h);
            h.u32(departure.workplace.0);
            h.u64(departure.tick);
        }
        h.u32(self.takings);
        h.u64(self.takings_day);
    }
}

fn unit_margin(world: &World, role: Role, kind: &str, live: bool) -> Option<i64> {
    let price = |kind: &str| {
        if live {
            village::market::unit_price(world, kind)
        } else {
            catalog::price_in(world, kind)
        }
    };
    let registry = world.resource::<npc_framework::crafting::RecipeRegistry>();
    let cost = catalog::recipes_for(world, role)
        .iter()
        .filter_map(|name| registry.recipes.get(*name))
        .filter_map(|recipe| {
            let output = recipe.outputs.iter().find(|output| output.kind == kind)?;
            let total = recipe.inputs.iter().try_fold(0_i64, |sum, input| {
                Some(sum + i64::from(price(&input.key)?) * i64::from(input.count))
            })?;
            Some(total / i64::from(output.count.max(1)))
        })
        .min()
        .unwrap_or(0);
    Some(i64::from(price(kind)?) - cost)
}

/// Realised unit margin of one good for a profession: the learned market price
/// (recent sales) minus input cost at current reference prices, relative to the
/// same margin at seed prices, in permille of the seed price. Zero in an
/// undisturbed market, negative in a glut, positive in a shortage.
pub fn good_margin_permille(world: &World, role: Role, kind: &str) -> i64 {
    let (Some(now), Some(seed), Some(base)) = (
        unit_margin(world, role, kind, true),
        unit_margin(world, role, kind, false),
        catalog::price_in(world, kind),
    ) else {
        return 0;
    };
    ((now - seed) * 1000 / i64::from(base.max(1))).clamp(-1000, 1000)
}

/// Best realised margin among the goods a profession can make.
pub fn margin_permille(world: &World, role: Role) -> i64 {
    catalog::outputs_for(world, role)
        .iter()
        .map(|kind| good_margin_permille(world, role, kind))
        .max()
        .unwrap_or(0)
}

struct LaborTuning {
    margin_gap: i64,
    margin_persist: u64,
    margin_cooldown: u64,
    switch_cooldown: u64,
    profession_return: u64,
    vacated_seat: u64,
}
fn labor_tuning(world: &World) -> LaborTuning {
    economy_spec::tuning(world).map_or(
        LaborTuning {
            margin_gap: MARGIN_GAP_PERMILLE,
            margin_persist: MARGIN_PERSIST_TICKS,
            margin_cooldown: MARGIN_COOLDOWN,
            switch_cooldown: SWITCH_COOLDOWN,
            profession_return: PROFESSION_RETURN_TICKS,
            vacated_seat: VACATED_SEAT_TICKS,
        },
        |tuning| LaborTuning {
            margin_gap: tuning.margin_gap_permille,
            margin_persist: tuning.margin_persist_ticks,
            margin_cooldown: tuning.margin_cooldown_ticks,
            switch_cooldown: tuning.switch_cooldown_ticks,
            profession_return: tuning.profession_return_ticks,
            vacated_seat: tuning.vacated_seat_ticks,
        },
    )
}

pub fn profile(world: &World, actor: Entity) -> Option<&ResidentProfile> {
    world.get::<ResidentProfile>(actor)
}

pub fn register(app: &mut App) {
    app.register_sim_component::<ResidentProfile>()
        .register_sim_component::<Employment>()
        .register_sim_component::<village::routing::RouteBlacklist>()
        .register_sim_component::<village::goap::PlanMemo>()
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

/// `sim_rand` purposes of the seeded founding variation (tick 0, per resident).
const FOOD_PREFERENCE_PURPOSE: u32 = 0x4543_4f01;
const ENDOWMENT_PURPOSE: u32 = 0x4543_4f02;
const APTITUDE_PURPOSE: u32 = 0x4543_4f03;

fn aptitudes(
    economy: &economy_spec::EconomySpec,
    role: Role,
    flexible: bool,
    seed: u64,
    id: SimId,
) -> Vec<Role> {
    // Every trade of the catalog, sorted by role (named trades, then data
    // trades by key): a flexible founder may take any of them.
    let mut every: Vec<Role> = economy.professions.iter().map(|entry| entry.role).collect();
    every.sort_unstable();
    let mut roles = if flexible {
        every.clone()
    } else {
        economy
            .profession(role)
            .map(|entry| entry.aptitudes.clone())
            .unwrap_or_default()
    };
    roles.sort_unstable();
    roles.dedup();
    // Seeded individuality: a founder has a knack for a few trades outside the
    // usual aptitudes of the profession. Skill gates still apply.
    for draw in 0..economy.seed.extra_aptitudes {
        let spare: Vec<_> = every
            .iter()
            .copied()
            .filter(|candidate| !roles.contains(candidate))
            .collect();
        if spare.is_empty() {
            break;
        }
        let pick =
            npc_framework::kernel::rng::sim_rand(seed, id, u64::from(draw), APTITUDE_PURPOSE);
        roles.push(spare[(pick % spare.len() as u64) as usize]);
        roles.sort_unstable();
    }
    roles
}

/// One finite initial endowment, used only during construction of a new world.
pub fn spawn(world: &mut World) {
    let tree = world
        .resource::<AiRegistry>()
        .tree_id(village::behavior::TREE)
        .expect("regional life tree");
    let economy = economy_spec::registry(world)
        .expect("regional economy")
        .spec
        .clone();
    let seed = world.resource::<WorldSeed>().0;
    let traits = village::resident::spec(world)
        .expect("regional behaviour data")
        .traits
        .clone();
    for (region_index, (settlement, _)) in settlements::all(world).into_iter().enumerate() {
        let definition = settlements::definition(world, settlement)
            .expect("settlement definition")
            .clone();
        for (index, base_name) in NAMES[region_index % NAMES.len()]
            .iter()
            .copied()
            .enumerate()
        {
            let founder = &economy.seed.founders[index];
            let role = founder.role;
            let trade = economy.profession(role).expect("founder profession");
            let home = &definition.homes[index];
            let home_entity =
                settlements::home_entity(world, settlement, &home.key).expect("resident home");
            let workplace =
                settlements::workplace(world, settlement, role).expect("resident workplace");
            // By default three of fourteen residents per settlement may take any trade.
            let flexible = founder.flexible;
            let template = trade
                .flexible_template
                .as_ref()
                .filter(|_| flexible)
                .unwrap_or(&trade.template);
            let spec = crate::layout::UnitSpec {
                template: template.clone(),
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
                        preferred_food: economy.seed.food_preferences
                            [(npc_framework::kernel::rng::sim_rand(
                                seed,
                                id,
                                0,
                                FOOD_PREFERENCE_PURPOSE,
                            ) % economy.seed.food_preferences.len() as u64)
                                as usize]
                            .clone(),
                        allowed_roles: aptitudes(&economy, role, flexible, seed, id),
                        flexible,
                    },
                    Employment {
                        last_reason: "Established at the village worksite".into(),
                        ..Default::default()
                    },
                    village::resident::seed_traits(&traits, seed, id),
                ));
            world
                .get_mut::<Wallet>(actor)
                .expect("resident wallet")
                .coins = economy.seed.coins;
            world
                .get_mut::<village::colony::Household>(home_entity)
                .expect("household")
                .residents
                .push(id);
            for (slot, stack) in economy.seed.common.iter().enumerate() {
                // Seeded band: households do not all start with identical stores.
                let bonus =
                    (npc_framework::kernel::rng::sim_rand(seed, id, slot as u64, ENDOWMENT_PURPOSE)
                        % (u64::from(economy.seed.endowment_bonus_max) + 1))
                        as u32;
                let count = stack.count + bonus;
                if count > 0 {
                    economy::grant(world, actor, Item::new(&stack.good).stack(count));
                }
            }
            for stack in &trade.endowment {
                if stack.count > 0 {
                    economy::grant(world, actor, Item::new(&stack.good).stack(stack.count));
                }
            }
            if let Some(tool) = &trade.tool {
                village::living::grant_tool(world, actor, tool);
            }
            let ordinal = world.resource::<crate::IslandIds>().units.len();
            village::living::found_wearing_coat(world, actor, ordinal);
            let mut ids = world.resource_mut::<crate::IslandIds>();
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

/// Settles one more resident in `settlement`: a real person with a trade, no
/// home, no coins and no goods, standing at the centre. They take the first
/// free seat of the settlement (in workplace identity order) and sleep in the
/// square until the civic planner finds or builds them a bed. Arrivals and
/// tests use this; it mints nothing.
pub fn settle_newcomer(world: &mut World, settlement: SimId, name: &str) -> Result<SimId, String> {
    let center = settlements::definition(world, settlement)
        .ok_or("unknown settlement")?
        .center;
    let id = settle(
        world,
        settlement,
        &Settler {
            name,
            at: center,
            seat: None,
            extra_roles: &[],
            reason: "Arrived and took a free place",
        },
    )?;
    if let Some(actor) = world.resource::<SimIndex>().get(id) {
        village::demography::enrolled(world, actor, id);
    }
    Ok(id)
}

/// Workplaces of a settlement with a free seat, in identity order. The inn is
/// never offered to a newcomer.
pub fn free_seats(world: &World, settlement: SimId) -> Vec<(SimId, Role)> {
    let taken = seats_taken(world);
    workplaces(world)
        .into_iter()
        .filter(|(_, entity)| settlements::member(world, *entity) == Some(settlement))
        .filter_map(|(id, _)| {
            let (_, role, capacity) = site(world, id)?;
            (service(world, role).is_none() && taken.get(&id).copied().unwrap_or(0) < capacity)
                .then_some((id, role))
        })
        .collect()
}

/// Seats and living workers of `role`'s workplaces in `settlement`.
pub fn seats_in(world: &World, settlement: SimId, role: Role) -> (u32, u32) {
    let taken = seats_taken(world);
    workplaces(world)
        .into_iter()
        .filter_map(|(id, entity)| {
            let (_, kind, capacity) = site(world, id)?;
            (kind == role && settlements::member(world, entity) == Some(settlement))
                .then(|| (capacity, taken.get(&id).copied().unwrap_or(0)))
        })
        .fold((0, 0), |(seats, held), (capacity, workers)| {
            (seats + capacity, held + workers)
        })
}

/// Whether the essential-coverage floor keeps the last worker of `role` in
/// `settlement`: an essential trade whose goods are still wanted there.
pub fn essential_kept(world: &World, settlement: SimId, role: Role) -> bool {
    essential(world, role) && shortage(world, settlement, role) > 0
}

/// The resident of `settlement` (working another trade) closest to the
/// skill `role` needs, when one has the aptitude but not the skill:
/// `(resident, name, skill, has, needs)`.
pub fn skill_gap(
    world: &World,
    settlement: SimId,
    role: Role,
) -> Option<(SimId, String, String, u32, u32)> {
    let skill = economy_spec::registry(world)?
        .definition(role)?
        .skill
        .clone()?;
    village::demography::members(world, settlement)
        .into_iter()
        .filter(|(_, actor)| {
            village::living(world, *actor)
                && world
                    .get::<Villager>(*actor)
                    .is_some_and(|person| person.role != role)
                && profile(world, *actor)
                    .is_some_and(|profile| profile.allowed_roles.contains(&role))
        })
        .filter_map(|(id, actor)| {
            let has = npc_framework::rules::effective_skill(world, id, &skill.name).unwrap_or(0);
            let name = world.get::<Villager>(actor)?.name.clone();
            (has < skill.level)
                .then(|| (id, name, skill.name.clone(), has as u32, skill.level as u32))
        })
        .max_by_key(|(id, _, _, has, _)| (*has, std::cmp::Reverse(*id)))
}

/// Whether a living resident of `settlement` working another trade may take
/// `role` (aptitude and skill).
pub fn anyone_may_take(world: &World, settlement: SimId, role: Role) -> bool {
    village::demography::members(world, settlement)
        .into_iter()
        .any(|(_, actor)| {
            village::living(world, actor)
                && world
                    .get::<Villager>(actor)
                    .is_some_and(|person| person.role != role)
                && qualified(world, actor, role)
        })
}

/// Every workplace, ascending by identity.
fn workplaces(world: &World) -> Vec<(SimId, Entity)> {
    let Some(mut query) = world.try_query::<(Entity, &SimId, &village::VillagePlace)>() else {
        return Vec::new();
    };
    let mut out: Vec<(SimId, Entity)> = query.iter(world).map(|(e, id, _)| (*id, e)).collect();
    out.sort_unstable_by_key(|(id, _)| *id);
    out
}

/// Living workers by workplace: one pass instead of one scan per workplace.
fn seats_taken(world: &World) -> std::collections::BTreeMap<SimId, u32> {
    let mut taken = std::collections::BTreeMap::new();
    if let Some(mut query) = world.try_query::<(Entity, &Villager)>() {
        for (entity, person) in query.iter(world) {
            if person.role != Role::Visitor && village::living(world, entity) {
                *taken.entry(person.workplace).or_default() += 1;
            }
        }
    }
    taken
}

/// Who settles: a name, where they stand, optionally the seat they take and
/// trades they may take besides the usual aptitudes of that seat.
pub struct Settler<'a> {
    pub name: &'a str,
    pub at: SimPos,
    /// `(workplace, profession)`; `None` takes the first free seat.
    pub seat: Option<(SimId, Role)>,
    pub extra_roles: &'a [Role],
    /// Recorded as the reason of the first employment.
    pub reason: &'a str,
}

/// [`settle_newcomer`] with the details chosen by the caller. Mints nothing.
pub fn settle(world: &mut World, settlement: SimId, settler: &Settler) -> Result<SimId, String> {
    let name = settler.name;
    let definition = settlements::definition(world, settlement)
        .ok_or("unknown settlement")?
        .clone();
    let (workplace, role) = match settler.seat {
        Some(seat) => seat,
        None => free_seats(world, settlement)
            .into_iter()
            .next()
            .ok_or("the settlement has no free workplace seat")?,
    };
    let economy = economy_spec::registry(world)
        .ok_or("regional economy is missing")?
        .spec
        .clone();
    let trade = economy.profession(role).ok_or("unknown profession")?;
    let tree = world
        .resource::<AiRegistry>()
        .tree_id(village::behavior::TREE)
        .ok_or("regional life tree is missing")?;
    let traits = village::resident::spec(world)
        .ok_or("regional behaviour data is missing")?
        .traits
        .clone();
    let seed = world.resource::<WorldSeed>().0;
    let spec = crate::layout::UnitSpec {
        template: trade.template.clone(),
        behaviour: crate::Behaviour::Idle,
        ..Default::default()
    };
    let id = crate::layout::spawn_unit(world, settler.at, &spec)?;
    let actor = world
        .resource::<SimIndex>()
        .get(id)
        .ok_or("newcomer identity is missing")?;
    village::initialise_person(world, actor, role, name, definition.center, workplace);
    let mut allowed_roles = aptitudes(&economy, role, false, seed, id);
    allowed_roles.extend(settler.extra_roles.iter().copied());
    allowed_roles.sort_unstable();
    allowed_roles.dedup();
    let key = format!("{}.newcomer.{}", definition.key, id.0);
    world
        .entity_mut(actor)
        .remove::<(BrainState, ActiveLeaf)>()
        .insert((
            Brain { tree },
            Inventory::new(64, 500_000),
            SettlementMember { settlement },
            ResidentProfile {
                key: key.clone(),
                appearance: (id.0 % 42) as u8,
                preferred_food:
                    economy.seed.food_preferences[(npc_framework::kernel::rng::sim_rand(
                        seed,
                        id,
                        0,
                        FOOD_PREFERENCE_PURPOSE,
                    ) % economy.seed.food_preferences.len() as u64)
                        as usize]
                        .clone(),
                allowed_roles,
                flexible: false,
            },
            Employment {
                last_reason: settler.reason.into(),
                ..Default::default()
            },
            village::resident::seed_traits(&traits, seed, id),
        ));
    let mut ids = world.resource_mut::<crate::IslandIds>();
    ids.name(key, id);
    ids.units.push(id);
    production::invalidate(world);
    record_effect(
        world,
        Some(id),
        "RegionalNewcomerSettled",
        &json!({"settlement":settlement,"role":role.name(),"workplace":workplace}),
    );
    if let Some(actor) = world.resource::<SimIndex>().get(id) {
        village::resident::assign_tree(world, actor);
    }
    Ok(id)
}

fn site(world: &World, workplace: SimId) -> Option<(Entity, Role, u32)> {
    let entity = world.resource::<SimIndex>().get(workplace)?;
    let role = world.get::<village::VillagePlace>(entity)?.role;
    let settlement = settlements::member(world, entity)?;
    // A workplace built after the founding carries its own seat count.
    if let Some(seats) = village::civic::workplace_seats(world, entity) {
        return Some((entity, role, seats));
    }
    let definition = settlements::definition(world, settlement)?;
    let key = &world.get::<super::SettlementSite>(entity)?.key;
    let capacity = definition
        .sites
        .iter()
        .find(|site| &site.key == key)?
        .capacity;
    Some((entity, role, capacity))
}

/// Seats of a workplace, authored or built; `None` for anything else.
pub fn capacity(world: &World, workplace: SimId) -> Option<u32> {
    site(world, workplace).map(|(_, _, capacity)| capacity)
}

pub fn assigned(world: &World, workplace: SimId) -> u32 {
    // A query over the people, not a scan of every identity in the world.
    let Some(mut query) = world.try_query::<(Entity, &Villager)>() else {
        return 0;
    };
    query
        .iter(world)
        .filter(|(entity, person)| {
            person.workplace == workplace
                && person.role != Role::Visitor
                && village::living(world, *entity)
        })
        .count() as u32
}

/// Every living actor, ascending by identity.
fn living_actors(world: &World) -> Vec<(SimId, Entity)> {
    let mut out = village::lookup::ids_with::<village::Vitality>(world);
    out.retain(|(_, entity)| village::living(world, *entity));
    out
}

fn qualified(world: &World, actor: Entity, role: Role) -> bool {
    if profile(world, actor).is_none_or(|profile| !profile.allowed_roles.contains(&role)) {
        return false;
    }
    let skill = economy_spec::registry(world)
        .and_then(|registry| registry.definition(role))
        .and_then(|entry| entry.skill.clone());
    skill.is_none_or(|skill| {
        world.get::<SimId>(actor).is_some_and(|id| {
            npc_framework::rules::effective_skill(world, *id, &skill.name)
                .is_some_and(|level| level >= skill.level)
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

fn essential(world: &World, role: Role) -> bool {
    economy_spec::registry(world)
        .and_then(|registry| registry.profession(role))
        .is_some_and(|entry| entry.essential)
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
    let required = economy_spec::registry(world)
        .and_then(|registry| registry.profession(role))
        .and_then(|entry| entry.required_tool);
    if required.is_some_and(|tool| !tool_obtainable(world, actor, tool)) {
        return false;
    }
    // Hunting is a physical activity on wildlife, not on a resource patch.
    if role == village::roles::roles(world).hunter {
        return village::hunting::available_prey(world)
            .into_iter()
            .any(|(_, _, position)| routes::estimate(world, here, position).is_some());
    }
    if !catalog::recipes_for(world, role).is_empty() || catalog::outputs_for(world, role).is_empty()
    {
        return true;
    }
    let patches: Vec<(Entity, &village::ResourcePatch)> = world
        .try_query::<(Entity, &village::ResourcePatch)>()
        .map(|mut query| query.iter(world).collect())
        .unwrap_or_default();
    catalog::outputs_for(world, role).iter().any(|kind| {
        patches.iter().any(|(entity, patch)| {
            patch.kind == *kind
                && patch.remaining > 0
                && world
                    .get::<SimPos>(*entity)
                    .is_some_and(|pos| routes::estimate(world, here, *pos).is_some())
        })
    }) || fields_viable(world, actor, role)
}

/// A farmer always has the fields of the settlement to work.
fn fields_viable(world: &World, actor: Entity, role: Role) -> bool {
    role == village::roles::roles(world).sower
        && production::economic_settlement(world, actor)
            .is_some_and(|settlement| !village::fields::fields_of(world, settlement).is_empty())
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
    let Some(mut query) = world.try_query::<(Entity, &Villager)>() else {
        return false;
    };
    query.iter(world).any(|(maker, person)| {
        catalog::outputs_for(world, person.role).contains(&kind)
            && village::living(world, maker)
            && production::economic_settlement(world, maker) == settlement
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
    staff_services(world);
    let mut people: Vec<(SimId, Entity)> = world
        .try_query::<(Entity, &SimId, &ResidentProfile)>()
        .map(|mut query| query.iter(world).map(|(e, id, _)| (*id, e)).collect())
        .unwrap_or_default();
    people.sort_unstable_by_key(|(id, _)| *id);
    let people: Vec<Entity> = people.into_iter().map(|(_, actor)| actor).collect();
    let taken = seats_taken(world);
    let open: Vec<(SimId, Role)> = workplaces(world)
        .into_iter()
        .filter_map(|(id, entity)| {
            let (_, role, capacity) = site(world, id)?;
            let settlement = settlements::member(world, entity)?;
            (service(world, role).is_none() && taken.get(&id).copied().unwrap_or(0) < capacity)
                .then_some((settlement, role))
        })
        .collect();
    let tuning = labor_tuning(world);
    let margins: std::collections::BTreeMap<Role, i64> = super::economy_spec::trades(world)
        .iter()
        .map(|role| (*role, margin_permille(world, *role)))
        .collect();
    for actor in people {
        let alive = village::living(world, actor);
        let viable = world.get::<Villager>(actor).is_some_and(|person| {
            viable(world, actor, person.role, person.workplace)
                && !idle_trade(world, actor, person.role)
        });
        let own = world.get::<Villager>(actor).map(|person| person.role);
        let settlement = settlements::member(world, actor);
        let gap = own.is_some_and(|own| {
            open.iter()
                .filter(|(region, role)| {
                    Some(*region) == settlement && *role != own && qualified(world, actor, *role)
                })
                .filter_map(|(_, role)| margins.get(role))
                .max()
                .is_some_and(|best| {
                    best - margins.get(&own).copied().unwrap_or(0) >= tuning.margin_gap
                })
        });
        let mut employment = world
            .get_mut::<Employment>(actor)
            .expect("resident employment");
        if !alive || !gap {
            employment.margin_gap_since = None;
        } else if employment.margin_gap_since.is_none() {
            employment.margin_gap_since = Some(now);
        }
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
    // Only the living hold or want goods.
    for (id, actor) in &living_actors(world) {
        if production::economic_settlement(world, *actor) != Some(settlement) {
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
    // Goods on the settlement's store shelves are supply too.
    for (_, store) in village::civic::depots(world, settlement) {
        if let Some(shelf) = world.get::<village::civic::Depot>(store) {
            for kind in outputs {
                stock = stock.saturating_add(shelf.stock(kind));
            }
        }
    }
    wanted.saturating_sub(stock)
}

/// Unmet funded demand for one good in a settlement, after the stock in its
/// residents' packs and on its store shelves.
pub fn good_shortage(world: &World, settlement: SimId, kind: &str) -> u32 {
    let Some(plan) = world.get_resource::<production::ProductionPlan>() else {
        return 0;
    };
    let mut wanted = 0_u32;
    let mut stock = 0_u32;
    for (id, actor) in village::demography::residents(world) {
        if production::economic_settlement(world, actor) != Some(settlement) {
            continue;
        }
        wanted = wanted.saturating_add(
            plan.procurement
                .get(&id)
                .and_then(|goods| goods.get(kind))
                .copied()
                .unwrap_or(0),
        );
        stock = stock.saturating_add(economy::count(world, actor, kind));
    }
    for (_, store) in village::civic::depots(world, settlement) {
        if let Some(shelf) = world.get::<village::civic::Depot>(store) {
            stock = stock.saturating_add(shelf.stock(kind));
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
    let taken = seats_taken(world);
    workplaces(world)
        .into_iter()
        .filter_map(|(id, entity)| {
            let (_, role, capacity) = site(world, id)?;
            let settlement = settlements::member(world, entity)?;
            let occupied = taken.get(&id).copied().unwrap_or(0);
            (occupied < capacity).then(|| Vacancy {
                workplace: id,
                settlement,
                role: role.name().into(),
                occupied,
                capacity,
                shortage: shortage(world, settlement, role),
            })
        })
        .collect()
}

/// A worker whose trade has no work: no funded production of any good it
/// makes is planned for it and nothing of it is short, while another
/// worker of the trade remains on the island. Such a worker counts as
/// blocked, so after `BLOCKED_TICKS` it may take a vacancy where work is
/// short; the trade's headcount follows the work, down to one.
fn idle_trade(world: &World, actor: Entity, role: Role) -> bool {
    if service(world, role).is_some() {
        return false;
    }
    // Only after `idle_trade_days` in the trade; 0 switches the rule off.
    let tenure = economy_spec::tuning(world).map_or(0, |tuning| tuning.idle_trade_days);
    if tenure == 0 {
        return false;
    }
    let day = world.resource::<SimConfig>().day_length.max(1);
    let since = world
        .get::<Employment>(actor)
        .map_or(0, |employment| employment.changed_at);
    if world.resource::<SimClock>().tick < since.saturating_add(u64::from(tenure) * day) {
        return false;
    }
    let outputs = catalog::outputs_for(world, role);
    let Some(settlement) = settlements::member(world, actor) else {
        return false;
    };
    // No work: nothing of the trade is wanted from this worker (no funded
    // production of any of its goods) and nothing of it is short.
    if outputs.is_empty()
        || outputs
            .iter()
            .any(|kind| production::production_demand(world, actor, kind) > 0)
        || shortage(world, settlement, role) > 0
    {
        return false;
    }
    living_actors(world)
        .into_iter()
        .filter(|(_, other)| {
            *other != actor
                && world
                    .get::<Villager>(*other)
                    .is_some_and(|person| person.role == role)
        })
        .count()
        > 0
}

/// The service terms of a trade (the inn), from data.
pub fn service(world: &World, role: Role) -> Option<super::economy_spec::ServiceDef> {
    super::economy_spec::registry(world)?
        .spec
        .profession(role)?
        .service
}

/// Books a guest's fee as the takings of the keeper today.
pub(crate) fn took(world: &mut World, keeper: Entity, coins: u32) {
    if world
        .get::<Villager>(keeper)
        .is_none_or(|person| service(world, person.role).is_none())
    {
        return;
    }
    let day = village::calendar(world).0;
    if let Some(mut employment) = world.get_mut::<Employment>(keeper) {
        if employment.takings_day != day {
            employment.takings_day = day;
            employment.takings = 0;
        }
        employment.takings = employment.takings.saturating_add(coins);
    }
}

/// Service buildings without a keeper get one: a resident of the
/// settlement who may take the trade, from a trade with more than one worker
/// at its workplace first. And once a day each keeper's takings of the day
/// before are topped up to the trade's `wage_floor` by the civic fund, when
/// the building is the only one of its trade in its settlement and the fund
/// may spend it.
fn staff_services(world: &mut World) {
    let day = village::calendar(world).0;
    let mut places: Vec<(SimId, Entity, Role)> = workplaces(world)
        .into_iter()
        .filter_map(|(id, entity)| {
            let (_, role, _) = site(world, id)?;
            service(world, role).map(|_| (id, entity, role))
        })
        .collect();
    places.sort_unstable_by_key(|(id, _, _)| *id);
    for (workplace, entity, role) in places {
        let Some(settlement) = settlements::member(world, entity) else {
            continue;
        };
        if assigned(world, workplace) > 0 {
            continue;
        }
        let candidate = living_actors(world)
            .into_iter()
            .filter(|(_, actor)| {
                settlements::member(world, *actor) == Some(settlement)
                    && world.get::<Villager>(*actor).is_some_and(|person| {
                        service(world, person.role).is_none()
                            && (assigned(world, person.workplace) > 1
                                || !essential(world, person.role))
                    })
                    && qualified(world, *actor, role)
                    && safe_boundary(world, *actor)
                    && world.get::<Employment>(*actor).is_some_and(|employment| {
                        world.resource::<SimClock>().tick >= employment.cooldown_until
                    })
            })
            .min_by_key(|(id, actor)| {
                let shared = world
                    .get::<Villager>(*actor)
                    .is_some_and(|person| assigned(world, person.workplace) > 1);
                (!shared, *id)
            });
        if let Some((_, actor)) = candidate {
            change(
                world,
                actor,
                role,
                workplace,
                "Keeps the inn: a service post nobody held",
            );
        }
    }
    // The wage floor, once a day, for yesterday.
    let mut keepers: Vec<(SimId, Entity, Role, SimId)> = living_actors(world)
        .into_iter()
        .filter_map(|(id, actor)| {
            let person = world.get::<Villager>(actor)?;
            service(world, person.role)?;
            Some((id, actor, person.role, person.workplace))
        })
        .collect();
    keepers.sort_unstable_by_key(|(id, _, _, _)| *id);
    for (id, actor, role, workplace) in keepers {
        let Some(terms) = service(world, role) else {
            continue;
        };
        let Some(employment) = world.get::<Employment>(actor).cloned() else {
            continue;
        };
        if employment.takings_day >= day {
            continue;
        }
        let takings = if employment.takings_day + 1 == day {
            employment.takings
        } else {
            0
        };
        {
            let mut employment = world.get_mut::<Employment>(actor).expect("employment");
            employment.takings_day = day;
            employment.takings = 0;
        }
        let settlement =
            site(world, workplace).and_then(|(place, _, _)| settlements::member(world, place));
        let only = settlement.is_some_and(|settlement| {
            workplaces(world)
                .into_iter()
                .filter(|(other, entity)| {
                    settlements::member(world, *entity) == Some(settlement)
                        && site(world, *other).is_some_and(|(_, other_role, _)| other_role == role)
                })
                .count()
                == 1
        });
        let due = terms.wage_floor.saturating_sub(takings);
        if !only || due == 0 || village::civic::budget(world) < due {
            continue;
        }
        world.resource_mut::<village::VillageState>().fund -= due;
        world.get_mut::<Wallet>(actor).expect("keeper wallet").coins += due;
        record_effect(
            world,
            Some(id),
            "VillageServiceWage",
            &json!({"keeper":id,"workplace":workplace,"takings":takings,"paid":due,
                "wage_floor":terms.wage_floor}),
        );
    }
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
    // A service keeper (the inn) stays while its building stands: its post
    // is not weighed against output margins.
    if service(world, person.role).is_some() && site(world, person.workplace).is_some() {
        return false;
    }
    let tuning = labor_tuning(world);
    // A margin gap that has persisted shortens the switching inertia.
    let margin_pull = employment
        .margin_gap_since
        .is_some_and(|since| now >= since.saturating_add(tuning.margin_persist));
    let cooldown_until = if margin_pull {
        employment
            .cooldown_until
            .min(employment.changed_at.saturating_add(tuning.margin_cooldown))
    } else {
        employment.cooldown_until
    };
    if now < cooldown_until || now < employment.next_review || now < REVIEW_TICKS {
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
    // Essential-coverage floor: the last worker of an essential profession stays
    // while the settlement it serves still wants the goods, even when the work
    // is blocked for now (an exhausted patch regrows; an abandoned yard does not).
    let served =
        site(world, person.workplace).and_then(|(place, _, _)| settlements::member(world, place));
    if essential(world, person.role)
        && !duplicate
        && (!blocked || served.is_some_and(|region| shortage(world, region, person.role) > 0))
    {
        return false;
    }
    // Otherwise only a blocked, surplus or flexible worker may move freely. A
    // settled specialist may still cover a vacant essential trade at home.
    let settlement = settlements::member(world, actor);
    // The overseer's targets (`oversee`): a worker of a trade over its target
    // is surplus; one of a trade at or under its target stays unless its work
    // is blocked; a specialist may answer a target elsewhere at home while
    // another worker of its own trade remains.
    let own_over =
        settlement.is_some_and(|region| super::oversee::over_target(world, region, person.role));
    let own_held = settlement.is_some_and(|region| {
        super::oversee::target(world, region, person.role)
            .is_some_and(|wanted| super::oversee::headcount(world, region, person.role) <= wanted)
    });
    let own_spare = own_over
        || !essential(world, person.role)
        || settlement
            .is_some_and(|region| super::oversee::headcount(world, region, person.role) > 1);
    let covering = !blocked
        && !duplicate
        && !own_over
        && profile(world, actor).is_none_or(|profile| !profile.flexible);
    let Some(here) = world.get::<SimPos>(actor).copied() else {
        return false;
    };
    let own_margin = margin_permille(world, person.role);
    let own_shortage = served.map_or(0, |region| shortage(world, region, person.role));
    // Hysteresis against thrash. Seats somebody left a moment ago stay open, so
    // opposite moves in one pass net out instead of swapping two residents.
    let vacated: BTreeSet<SimId> = world
        .try_query::<&Employment>()
        .map(|mut query| {
            query
                .iter(world)
                .filter_map(|employment| employment.departures.last())
                .filter(|departure| now < departure.tick.saturating_add(tuning.vacated_seat))
                .map(|departure| departure.workplace)
                .collect()
        })
        .unwrap_or_default();
    let mut candidates = Vec::new();
    for vacancy in vacancies(world) {
        if vacancy.workplace == person.workplace {
            continue;
        }
        let Ok(role) = legal_assignment(world, actor, vacancy.workplace) else {
            continue;
        };
        if service(world, role).is_some() {
            continue;
        }
        if !viable(world, actor, role, vacancy.workplace) {
            continue;
        }
        let local = settlement == Some(vacancy.settlement);
        // A trade under the overseer's target in the worker's own settlement.
        let wished =
            local && own_spare && super::oversee::below_target(world, vacancy.settlement, role) > 0;
        let essential_gap = vacancy.occupied == 0 && essential(world, role);
        if !essential_gap
            && (super::oversee::at_or_over_target(world, vacancy.settlement, role)
                || (own_held && !blocked))
        {
            continue;
        }
        if covering && !((essential_gap && local) || wished) {
            continue;
        }
        let gap = margin_permille(world, role) - own_margin;
        let pulled = (margin_pull && gap >= tuning.margin_gap) || wished;
        // An empty essential workplace overrides the hysteresis rules. A
        // return to a trade left recently is otherwise allowed only on strong
        // price evidence: a persistent pay gap of twice the usual threshold.
        let floor = vacancy.occupied == 0 && essential(world, role);
        let strong_pull = pulled && gap >= tuning.margin_gap.saturating_mul(2);
        let returning = employment.departures.iter().any(|departure| {
            departure.role == role && now < departure.tick.saturating_add(tuning.profession_return)
        });
        if !floor && (vacated.contains(&vacancy.workplace) || (returning && !strong_pull)) {
            continue;
        }
        // Adjustment, not musical chairs: a worker whose own goods are short
        // moves only towards a larger unmet demand.
        if !floor && !blocked && !pulled && own_shortage > 0 && vacancy.shortage <= own_shortage {
            continue;
        }
        // A vacancy can be valuable before the missing profession has produced
        // any profit. Otherwise require a genuine shortage or a persistent
        // realised-margin gap, not empty capacity.
        if vacancy.occupied > 0 && vacancy.shortage == 0 && !blocked && !pulled {
            continue;
        }
        if !blocked
            && !duplicate
            && !pulled
            && service(world, person.role).is_none()
            && vacancy.occupied > 0
        {
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
        let urgency = if vacancy.occupied == 0 && essential(world, role) {
            100_000_u64
        } else if wished {
            60_000
        } else if vacancy.occupied == 0 {
            50_000
        } else {
            10_000
        };
        let score = urgency
            .saturating_add(u64::from(vacancy.shortage.min(50)) * 100)
            .saturating_add(if local { 5000 } else { 0 })
            .saturating_sub(u64::from(route.distance_mm / 20));
        // Realised margin ranks otherwise comparable work: ten points per permille.
        let score = (score as i64).saturating_add(gap * 10).max(0) as u64;
        // Hysteresis also protects useful nonessential specialist work.
        if score > 15_000 || blocked || pulled {
            candidates.push((score, vacancy.workplace, role, pulled));
        }
    }
    candidates.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    let Some((_, workplace, role, pulled)) = candidates.into_iter().next() else {
        return false;
    };
    let wanted_move = settlements::member(world, actor)
        .is_some_and(|region| super::oversee::below_target(world, region, role) > 0);
    change(
        world,
        actor,
        role,
        workplace,
        if blocked {
            "Resources or access unavailable; taking useful work"
        } else if wanted_move {
            "Answering the overseer's wanted headcount"
        } else if covering {
            "Covering a vacant essential village occupation"
        } else if pulled {
            "Following better-paid work after a persistent margin gap"
        } else {
            "Filling an undersupplied village occupation"
        },
    )
}

fn change(world: &mut World, actor: Entity, role: Role, workplace: SimId, reason: &str) -> bool {
    let now = world.resource::<SimClock>().tick;
    let previous = world.get::<Villager>(actor).expect("resident").clone();
    let tuning = labor_tuning(world);
    let switch_cooldown = tuning.switch_cooldown;
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
    // A trade may run its own tree (S3.1).
    village::resident::assign_tree(world, actor);
    world.entity_mut(actor).remove::<MoveIntent>();
    production::clear_intent(world, actor);
    let mut departures = world
        .get::<Employment>(actor)
        .map(|employment| employment.departures.clone())
        .unwrap_or_default();
    departures.retain(|departure| now < departure.tick.saturating_add(tuning.profession_return));
    departures.push(Departure {
        role: previous.role,
        workplace: previous.workplace,
        tick: now,
    });
    if departures.len() > MAX_DEPARTURES {
        departures.remove(0);
    }
    *world.get_mut::<Employment>(actor).expect("employment") = Employment {
        changed_at: now,
        cooldown_until: now.saturating_add(switch_cooldown),
        next_review: now.saturating_add(REVIEW_TICKS),
        blocked_since: None,
        pending: None,
        last_reason: reason.into(),
        margin_gap_since: None,
        departures,
        takings: 0,
        takings_day: 0,
    };
    production::invalidate(world);
    let id = *world.get::<SimId>(actor).expect("resident identity");
    // Keys for tools, display names for people.
    let from_trade = economy_spec::trade_label(world, previous.role);
    let to_trade = economy_spec::trade_label(world, role);
    record_effect(
        world,
        Some(id),
        "RegionalOccupationChanged",
        &json!({
            "from":previous.role.name(), "to":role.name(), "from_workplace":previous.workplace,
            "workplace":workplace, "reason":reason,
            "from_key":previous.role.key(), "to_key":role.key(),
            "from_trade":from_trade, "to_trade":to_trade,
            // The overseer asked for it: an assignment or a wanted headcount.
            "by_order": reason == "Requested workplace assignment"
                || reason == "Answering the overseer's wanted headcount",
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
        role: economy_spec::trade_label(world, person.role).into(),
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
            || employment.margin_gap_since.is_some_and(|tick| tick > now)
            || employment.departures.len() > MAX_DEPARTURES
            || employment
                .departures
                .iter()
                .any(|departure| departure.tick > now)
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
    // Newcomers may join the founders; a founder may never go missing. One
    // who died and was laid to rest is on record among the departed.
    let laid_to_rest = crate::village::demography::departed(world)
        .iter()
        .filter(|gone| gone.resident.is_some() && gone.laid_to_rest)
        .count();
    if keys.len() + laid_to_rest < settlements::all(world).len() * 14 {
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
