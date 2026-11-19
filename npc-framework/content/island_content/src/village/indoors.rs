//! Being inside a building (regional profile only).
//!
//! A resident who stands at the door of the home it sleeps in, and is not
//! walking, is inside that home: resting, sleeping, warming at the hearth,
//! eating at home and sheltering from a storm all happen indoors. The same
//! holds for a guest who has rented a bed at the inn. [`Indoors`] is that
//! state. While it names a building the resident is sheltered by the home's
//! hearth instead of exposed to the weather, and only somebody inside the
//! same building can meet it. Walking off, losing the bed or dying ends it.
//!
//! The resident's position stays the entrance point: the building's body is
//! solid, and navigation starts from the door. Views read [`inside`] and hide
//! or draw the resident indoors.
use super::civic::Building;
use super::colony::{self, Household};
use super::commands::HOME_REST_REACH_MM;
use super::frontier::settlements::{HOME_OFFSET_MM, SettlementMember, WORKPLACE_OFFSET_MM};
use super::resident::BlockKind;
use super::{Role, VillagePlace, Villager, living};
use bevy_app::App;
use bevy_ecs::prelude::*;
use bevy_reflect::Reflect;
use npc_framework::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::json;

/// A resident leaves a building when it is this far from its entrance.
const LEAVE_REACH_MM: i64 = 1_200;
/// A guest is inside the inn when the bed is rented this close to it.
const INN_REACH_MM: i64 = 8_000;
/// Going indoors is journaled only after this long outdoors.
const JOURNAL_AFTER_TICKS: u64 = 600;
/// Half extents of a founding home's body, as its blocker is spawned.
const FOUNDING_HALF_X_MM: i32 = 1_900;
const FOUNDING_HALF_Z_MM: i32 = 2_300;
/// Half extent of a founding workplace's body, as its blocker is spawned.
const WORKPLACE_HALF_MM: i32 = 2_500;
/// Guest beds of an inn: a bed is rented only while one is free.
pub const INN_BEDS: u32 = 4;

/// Where a building is: the point residents stand on to go in, the body that
/// blocks walking, and the doorway on the body's wall.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Geometry {
    entrance: SimPos,
    /// Body rectangle in millimetres: `lo` inclusive, `hi` exclusive.
    lo: SimPos,
    hi: SimPos,
    /// Quarter turns: the body lies towards +z, +x, -z or -x of the entrance.
    facing: u8,
    built: bool,
}
impl Geometry {
    /// The body as an inclusive rectangle of cells.
    fn cells(&self) -> ((i32, i32), (i32, i32)) {
        (
            (self.lo.x.div_euclid(1000), self.lo.z.div_euclid(1000)),
            (
                (self.hi.x - 1).div_euclid(1000),
                (self.hi.z - 1).div_euclid(1000),
            ),
        )
    }
    /// Centre of the doorway: the point of the wall nearest the entrance.
    fn door(&self) -> SimPos {
        match self.facing % 4 {
            0 => SimPos::new(self.entrance.x, self.lo.z),
            1 => SimPos::new(self.lo.x, self.entrance.z),
            2 => SimPos::new(self.entrance.x, self.hi.z),
            _ => SimPos::new(self.hi.x, self.entrance.z),
        }
    }
}

/// Geometry of a home or a workplace, from what the simulation blocks: a
/// placed building's plot, a homestead cottage's footprint, or the blocker a
/// founding home or workplace was spawned with.
fn geometry(world: &World, entity: Entity) -> Option<Geometry> {
    if let Some(building) = world.get::<Building>(entity) {
        let plot = &building.plot;
        return Some(Geometry {
            entrance: plot.door(),
            lo: SimPos::new(plot.lo_x * 1000, plot.lo_z * 1000),
            hi: SimPos::new((plot.hi_x + 1) * 1000, (plot.hi_z + 1) * 1000),
            facing: plot.rotation,
            built: true,
        });
    }
    let around =
        |entrance: SimPos, centre: SimPos, half_x: i32, half_z: i32, built: bool| Geometry {
            entrance,
            lo: SimPos::new(centre.x - half_x, centre.z - half_z),
            hi: SimPos::new(centre.x + half_x, centre.z + half_z),
            facing: if centre.z < entrance.z { 2 } else { 0 },
            built,
        };
    if let Some(household) = world.get::<Household>(entity) {
        // A homestead cottage: the entity is the body, the door is the plot.
        if let Some((centre, body)) = super::homestead::built_body(world, entity) {
            return Some(around(
                household.door,
                centre,
                body.half_x_mm,
                body.half_z_mm,
                true,
            ));
        }
        return Some(around(
            household.door,
            SimPos::new(household.door.x, household.door.z + HOME_OFFSET_MM),
            FOUNDING_HALF_X_MM,
            FOUNDING_HALF_Z_MM,
            false,
        ));
    }
    let entrance = *world.get::<SimPos>(entity)?;
    world.get::<VillagePlace>(entity)?;
    Some(around(
        entrance,
        SimPos::new(entrance.x, entrance.z + WORKPLACE_OFFSET_MM),
        WORKPLACE_HALF_MM,
        WORKPLACE_HALF_MM,
        false,
    ))
}

/// Whether, and where, a resident is indoors.
#[derive(
    Component, Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Reflect,
)]
#[reflect(Component)]
pub struct Indoors {
    /// The building the resident is inside; `None` outdoors.
    pub building: Option<SimId>,
    /// Tick of the last change.
    pub since: u64,
    /// Asleep in a bed until this tick unless a need wakes the resident; 0
    /// when not asleep.
    #[serde(default)]
    pub asleep_until: u64,
    /// The guest bed taken at an inn; meaningless in a home.
    #[serde(default)]
    pub bed: u32,
}
impl SimHash for Indoors {
    const NAME: &'static str = "regional_indoors_v1";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.opt(self.building.as_ref(), |h, id| h.u32(id.0));
        h.u64(self.since);
        h.u64(self.asleep_until);
        h.u32(self.bed);
    }
}

/// Whether the resident is asleep in a bed for the night.
pub fn asleep(world: &World, actor: Entity) -> bool {
    let now = world.resource::<SimClock>().tick;
    world
        .get::<Indoors>(actor)
        .is_some_and(|state| state.asleep_until > now)
        && world
            .get::<Villager>(actor)
            .is_some_and(|person| person.resting_until > now)
}

/// Ticks left of the resident's own sleep block; `None` outside it.
fn night_left(world: &World, actor: Entity) -> Option<u64> {
    if !super::resident::in_block(world, actor, BlockKind::Sleep) {
        return None;
    }
    let at = super::resident::day_permille(world);
    let end = super::resident::schedule_of(world, actor)
        .iter()
        .filter(|block| block.kind == BlockKind::Sleep)
        .map(|block| (block.to - at).rem_euclid(1000))
        .filter(|left| *left > 0)
        .min()
        .unwrap_or(1000 - at);
    let day = world.resource::<SimConfig>().day_length.max(1);
    Some((end as u64 * day).div_ceil(1000).max(1))
}

/// A resident at the door of its own home in its sleep block goes to bed and
/// sleeps to the end of the block. Returns false when it is not night for
/// this resident or it has no home here.
pub(super) fn fall_asleep(world: &mut World, actor: Entity) -> bool {
    let Some(rate) = super::frontier::economy_spec::tuning(world)
        .map(|tuning| tuning.sleep_energy_per_tick as i32)
    else {
        return false;
    };
    let Some(id) = world.get::<SimId>(actor).copied() else {
        return false;
    };
    let (Some(left), Some(_)) = (night_left(world, actor), colony::home_of(world, id)) else {
        return false;
    };
    let now = world.resource::<SimClock>().tick;
    super::ai::start_rest(world, actor, left, rate);
    if world.get::<Indoors>(actor).is_none() {
        world.entity_mut(actor).insert(Indoors::default());
    }
    if let Some(mut state) = world.get_mut::<Indoors>(actor) {
        state.asleep_until = now + left;
    }
    true
}

/// Puts an actor to bed until `until`: the played character's sleep to the
/// morning, and a guest's night at the inn.
pub(super) fn sleep_until(world: &mut World, actor: Entity, until: u64) {
    let Some(rate) = super::frontier::economy_spec::tuning(world)
        .map(|tuning| tuning.sleep_energy_per_tick as i32)
    else {
        return;
    };
    let now = world.resource::<SimClock>().tick;
    if until <= now {
        return;
    }
    super::ai::start_rest(world, actor, until - now, rate);
    if world.get::<Indoors>(actor).is_none() {
        world.entity_mut(actor).insert(Indoors::default());
    }
    if let Some(mut state) = world.get_mut::<Indoors>(actor) {
        state.asleep_until = until;
    }
}

/// Guest beds of an inn: `tuning.inn_guest_beds`, [`INN_BEDS`] elsewhere.
pub fn inn_beds(world: &World) -> u32 {
    super::frontier::economy_spec::tuning(world).map_or(INN_BEDS, |tuning| tuning.inn_guest_beds)
}

/// Wakes a sleeper: a need or an order cannot wait for the morning.
pub(super) fn wake(world: &mut World, actor: Entity) {
    if !asleep(world, actor) {
        return;
    }
    let now = world.resource::<SimClock>().tick;
    if let Some(mut person) = world.get_mut::<Villager>(actor) {
        person.resting_until = now;
    }
    if let Some(mut state) = world.get_mut::<Indoors>(actor) {
        state.asleep_until = 0;
    }
    if let Some(id) = world.get::<SimId>(actor).copied() {
        record_effect(world, Some(id), "VillageWoken", &json!({"tick":now}));
    }
}

/// Regional profile only.
pub fn register(app: &mut App) {
    app.register_sim_component::<Indoors>();
}

/// The building this actor is inside. `None` outdoors and in other profiles.
pub fn inside_entity(world: &World, actor: Entity) -> Option<SimId> {
    world.get::<Indoors>(actor).and_then(|state| state.building)
}

/// The building a resident is inside: its household, or the inn.
pub fn inside(world: &World, resident: SimId) -> Option<SimId> {
    inside_entity(world, world.resource::<SimIndex>().get(resident)?)
}

/// Everybody inside a building, ascending by identity.
pub fn occupants(world: &World, building: SimId) -> Vec<SimId> {
    let Some(mut query) = world.try_query::<(&SimId, &Indoors)>() else {
        return Vec::new();
    };
    let mut out: Vec<SimId> = query
        .iter(world)
        .filter(|(_, state)| state.building == Some(building))
        .map(|(id, _)| *id)
        .collect();
    out.sort_unstable();
    out
}

/// Entrance of a building somebody may be inside: a home's door, or the inn.
fn entrance(world: &World, building: SimId) -> Option<SimPos> {
    let entity = world.resource::<SimIndex>().get(building)?;
    world
        .get::<Household>(entity)
        .map(|home| home.door)
        .or_else(|| world.get::<SimPos>(entity).copied())
}

fn set(world: &mut World, actor: Entity, building: Option<SimId>, now: u64) {
    if let Some(mut state) = world.get_mut::<Indoors>(actor) {
        state.building = building;
        state.since = now;
    }
}

/// A guest who rents a bed beside the inn sleeps inside it.
/// Living world: `actor` stands at the inn `inn` (within the reach in which a
/// guest who rents a bed is inside it).
pub(crate) fn at_inn(world: &World, actor: Entity, inn: SimId) -> bool {
    let (Some(here), Some(door)) = (world.get::<SimPos>(actor).copied(), entrance(world, inn))
    else {
        return false;
    };
    here.dist2(door) <= INN_REACH_MM.pow(2)
}

pub(super) fn rented_bed(world: &mut World, actor: Entity, keeper: Entity) {
    let Some(inn) = world.get::<Villager>(keeper).map(|person| person.workplace) else {
        return;
    };
    let (Some(here), Some(door)) = (world.get::<SimPos>(actor).copied(), entrance(world, inn))
    else {
        return;
    };
    if here.dist2(door) > INN_REACH_MM.pow(2) {
        return;
    }
    let now = world.resource::<SimClock>().tick;
    if world.get::<Indoors>(actor).is_none() {
        world.entity_mut(actor).insert(Indoors::default());
    }
    // A guest who rents again keeps the bed.
    let bed = world
        .get::<Indoors>(actor)
        .filter(|state| state.building == Some(inn))
        .map(|state| state.bed)
        .or_else(|| free_guest_bed(world, inn))
        .unwrap_or(0);
    set(world, actor, Some(inn), now);
    if let Some(mut state) = world.get_mut::<Indoors>(actor) {
        state.bed = bed;
    }
    // In the guest's own sleep block the rented bed is a night's sleep.
    if let Some(left) = night_left(world, actor) {
        sleep_until(world, actor, now + left);
    }
}

/// The lowest guest bed of an inn nobody is in; `None` when it is full.
pub fn free_guest_bed(world: &World, inn: SimId) -> Option<u32> {
    let taken: Vec<u32> = world
        .try_query::<&Indoors>()
        .map(|mut query| {
            query
                .iter(world)
                .filter(|state| state.building == Some(inn))
                .map(|state| state.bed)
                .collect()
        })
        .unwrap_or_default();
    (0..inn_beds(world)).find(|bed| !taken.contains(bed))
}

/// Once per tick in the regional Effects pass: residents who stand at their
/// own door go inside, and those who walk off, lose the bed or die come out.
pub fn tick(world: &mut World) {
    let now = world.resource::<SimClock>().tick;
    let mut people: Vec<(SimId, Entity)> = world
        .query::<(Entity, &SimId, &Villager)>()
        .iter(world)
        .map(|(entity, id, _)| (*id, entity))
        .collect();
    people.sort_unstable_by_key(|(id, _)| *id);
    for (id, actor) in people {
        let Some(state) = world.get::<Indoors>(actor).copied() else {
            world.entity_mut(actor).insert(Indoors::default());
            continue;
        };
        let alive = living(world, actor);
        // The night is over, or the rest was cut short.
        if state.asleep_until != 0
            && world
                .get::<Villager>(actor)
                .is_none_or(|person| person.resting_until <= now || !alive)
            && let Some(mut state) = world.get_mut::<Indoors>(actor)
        {
            state.asleep_until = 0;
        }
        // A player order does not wait for the morning.
        if asleep(world, actor) && npc_framework::command::cond_has_unit_order(world, actor) {
            wake(world, actor);
        }
        let moving = world.get::<MoveIntent>(actor).is_some();
        let Some(here) = world.get::<SimPos>(actor).copied() else {
            continue;
        };
        match state.building {
            Some(building) => {
                let resting = world
                    .get::<Villager>(actor)
                    .is_some_and(|person| person.resting_until > now);
                let entity = world.resource::<SimIndex>().get(building);
                let home = entity.and_then(|entity| world.get::<Household>(entity));
                let stays = match home {
                    // A home: while the resident sleeps there and is at its door.
                    Some(home) => {
                        home.residents.contains(&id)
                            && here.dist2(home.door) <= LEAVE_REACH_MM.pow(2)
                    }
                    // The inn: for as long as the rented rest lasts.
                    None => resting,
                };
                if !alive || moving || !stays {
                    set(world, actor, None, now);
                }
            }
            None => {
                let Some(home) = world.get::<Villager>(actor).map(|person| person.home) else {
                    continue;
                };
                if !alive || moving || here.dist2(home) > i64::from(HOME_REST_REACH_MM).pow(2) {
                    continue;
                }
                // At the door of the home it sleeps in, and not passing by.
                let house = colony::home_of(world, id).filter(|house| {
                    world
                        .resource::<SimIndex>()
                        .get(*house)
                        .and_then(|entity| world.get::<Household>(entity))
                        .is_some_and(|household| household.door == home)
                });
                let Some(house) = house else {
                    continue;
                };
                set(world, actor, Some(house), now);
                if now.saturating_sub(state.since) >= JOURNAL_AFTER_TICKS {
                    record_effect(
                        world,
                        Some(id),
                        "VillageWentIndoors",
                        &json!({"building":house,"outdoors_ticks":now - state.since}),
                    );
                }
            }
        }
    }
}

/// What a resident at rest is doing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Repose {
    /// Not resting.
    Awake,
    /// Resting awake: by the hearth, on the ground, or in a bed by day.
    Resting,
    /// In a bed at night or in the resident's own sleep block.
    Asleep,
}

/// Whether a resident is awake, resting awake or asleep.
pub fn repose(world: &World, actor: Entity) -> Repose {
    let now = world.resource::<SimClock>().tick;
    let Some(person) = world.get::<Villager>(actor) else {
        return Repose::Awake;
    };
    if !living(world, actor) || person.resting_until <= now {
        return Repose::Awake;
    }
    // Regional: asleep is the night's sleep in a bed, whatever the hour
    // it started; any other rest is awake.
    if asleep(world, actor) && inside_entity(world, actor).is_some() {
        Repose::Asleep
    } else {
        Repose::Resting
    }
}

/// One bed of a home and who sleeps in it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct BedView {
    pub index: u32,
    /// The resident whose bed this is; `None` for a child's or a free bed.
    pub resident: Option<SimId>,
    /// The child whose bed this is (its number in the demography).
    pub child: Option<u32>,
    /// Empty for a free bed.
    pub name: String,
    /// The resident is inside the home and resting now.
    pub in_bed: bool,
    pub asleep: bool,
}

/// A home as a place: where its entrance is, what its body covers, which way
/// it faces, its beds and who is inside.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct HomeView {
    pub house: SimId,
    pub label: String,
    pub settlement: Option<SimId>,
    /// Where residents stand to go in, and its cell (metres).
    pub entrance: SimPos,
    pub entrance_cell: (i32, i32),
    /// Centre of the doorway in millimetres: the point of the body's wall in
    /// front of the entrance. Draw the door here; on a body of even width it
    /// is half a cell beside the middle of the wall.
    pub door: SimPos,
    /// Inclusive body rectangle in cells: the cells the simulation blocks.
    pub body_lo: (i32, i32),
    pub body_hi: (i32, i32),
    /// The same body in millimetres: `body_min` inclusive, `body_max` exclusive.
    pub body_min: SimPos,
    pub body_max: SimPos,
    /// Quarter turns: the body lies towards +z, +x, -z or -x of the entrance
    /// (0 to 3), as in `civic::Plot::rotation`. The door faces the other way.
    pub facing: u8,
    /// False for a home the world was founded with.
    pub built: bool,
    pub capacity: u32,
    /// One entry per bed, in bed order: living residents by identity, then
    /// the children of the home by number, then free beds.
    pub beds: Vec<BedView>,
    /// Members beyond the last bed, who share: residents, then children.
    pub without_bed: Vec<String>,
    /// Everybody inside now, ascending by identity.
    pub occupants: Vec<SimId>,
    pub heated: bool,
}

/// One home; `None` when `house` is not a household.
pub fn home(world: &World, house: SimId) -> Option<HomeView> {
    let entity = world.resource::<SimIndex>().get(house)?;
    let household = world.get::<Household>(entity)?;
    let now = world.resource::<SimClock>().tick;
    let cell = |pos: SimPos| (pos.x.div_euclid(1000), pos.z.div_euclid(1000));
    let place = geometry(world, entity)?;
    let (entrance, facing, built) = (place.entrance, place.facing, place.built);
    let (body_lo, body_hi) = place.cells();
    let occupants = occupants(world, house);
    let index = world.resource::<SimIndex>();
    let mut sleepers: Vec<(Option<SimId>, Option<u32>, String)> = household
        .residents
        .iter()
        .filter_map(|id| {
            let actor = index.get(*id).filter(|actor| living(world, *actor))?;
            let name = world.get::<Villager>(actor)?.name.clone();
            Some((Some(*id), None, name))
        })
        .collect();
    sleepers.extend(
        super::demography::children_of(world, house)
            .into_iter()
            .map(|child| (None, Some(child.id), child.name.clone())),
    );
    let mut beds = Vec::new();
    let mut without_bed = Vec::new();
    for (position, (resident, child, name)) in sleepers.into_iter().enumerate() {
        if position as u32 >= household.capacity {
            without_bed.push(name);
            continue;
        }
        let actor = resident.and_then(|id| index.get(id));
        let state = actor.map_or(Repose::Awake, |actor| repose(world, actor));
        beds.push(BedView {
            index: position as u32,
            resident,
            child,
            name,
            in_bed: state != Repose::Awake && resident.is_some_and(|id| occupants.contains(&id)),
            asleep: state == Repose::Asleep,
        });
    }
    for free in beds.len() as u32..household.capacity {
        beds.push(BedView {
            index: free,
            resident: None,
            child: None,
            name: String::new(),
            in_bed: false,
            asleep: false,
        });
    }
    Some(HomeView {
        house,
        label: household.label.clone(),
        settlement: world.get::<SettlementMember>(entity).map(|m| m.settlement),
        entrance,
        entrance_cell: cell(entrance),
        door: place.door(),
        body_lo,
        body_hi,
        body_min: place.lo,
        body_max: place.hi,
        facing,
        built,
        capacity: household.capacity,
        beds,
        without_bed,
        occupants,
        heated: household.heated_until > now,
    })
}

/// Every home, ascending by identity.
pub fn homes(world: &World) -> Vec<HomeView> {
    let Some(mut query) = world.try_query::<(&SimId, &Household)>() else {
        return Vec::new();
    };
    let mut ids: Vec<SimId> = query.iter(world).map(|(id, _)| *id).collect();
    ids.sort_unstable();
    ids.into_iter().filter_map(|id| home(world, id)).collect()
}

/// A workplace as the simulation places it: founding or built.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct WorkplaceView {
    pub building: SimId,
    pub label: String,
    pub role: Role,
    pub settlement: Option<SimId>,
    /// Where workers stand to go in (the workplace's position).
    pub entrance: SimPos,
    /// Centre of the doorway on the body's wall.
    pub door: SimPos,
    /// Body in millimetres: `body_min` inclusive, `body_max` exclusive.
    pub body_min: SimPos,
    pub body_max: SimPos,
    /// Quarter turns: the body lies towards +z, +x, -z or -x of the entrance.
    pub facing: u8,
    /// False for a workplace the world was founded with.
    pub built: bool,
    /// Whether the body blocks walking: an open well or fishing hut does not.
    pub solid: bool,
}

/// One workplace; `None` when `building` is not one.
pub fn workplace(world: &World, building: SimId) -> Option<WorkplaceView> {
    let entity = world.resource::<SimIndex>().get(building)?;
    let place = world.get::<VillagePlace>(entity)?;
    world.get::<Household>(entity).is_none().then_some(())?;
    let geometry = geometry(world, entity)?;
    // A founding workplace's body is a separate blocker entity behind the
    // entrance (`frontier::settlements::spawn`): every one but the open well
    // and the fishing spot has one. A placed building says it in its plot.
    let solid = world.get::<Building>(entity).map_or_else(
        || {
            world.get::<GridBlocker>(entity).is_some()
                || (world
                    .get::<super::frontier::SettlementSite>(entity)
                    .is_some()
                    && !super::frontier::economy_spec::open_site(world, place.role))
        },
        |b| b.plot.solid,
    );
    Some(WorkplaceView {
        building,
        label: place.label.clone(),
        role: place.role,
        settlement: world.get::<SettlementMember>(entity).map(|m| m.settlement),
        entrance: geometry.entrance,
        door: geometry.door(),
        body_min: geometry.lo,
        body_max: geometry.hi,
        facing: geometry.facing,
        built: geometry.built,
        solid,
    })
}

/// Every workplace, founding or built, ascending by identity.
pub fn workplaces(world: &World) -> Vec<WorkplaceView> {
    let Some(mut query) = world.try_query::<(&SimId, &VillagePlace)>() else {
        return Vec::new();
    };
    let mut ids: Vec<SimId> = query.iter(world).map(|(id, _)| *id).collect();
    ids.sort_unstable();
    ids.into_iter()
        .filter_map(|id| workplace(world, id))
        .collect()
}

/// The bed of a resident: `(home, bed index)`; `None` without a bed.
pub fn bed_of(world: &World, resident: SimId) -> Option<(SimId, u32)> {
    let house = colony::home_of(world, resident)?;
    home(world, house)?
        .beds
        .into_iter()
        .find(|bed| bed.resident == Some(resident))
        .map(|bed| (house, bed.index))
}

/// One guest bed of an inn and who has rented it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct GuestBedView {
    pub index: u32,
    /// The guest in the bed; `None` for a free bed.
    pub guest: Option<SimId>,
    /// Empty for a free bed.
    pub name: String,
    /// Tick the rented rest ends; 0 for a free bed.
    pub until: u64,
}

/// An inn as a place, like [`HomeView`]: entrance, doorway, body, facing,
/// guest beds and who is inside.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct InnView {
    pub inn: SimId,
    pub label: String,
    pub settlement: Option<SimId>,
    /// The living innkeeper who works here, if any.
    pub keeper: Option<SimId>,
    pub entrance: SimPos,
    pub entrance_cell: (i32, i32),
    /// Centre of the doorway in millimetres, as in [`HomeView::door`].
    pub door: SimPos,
    pub body_lo: (i32, i32),
    pub body_hi: (i32, i32),
    pub body_min: SimPos,
    pub body_max: SimPos,
    pub facing: u8,
    /// False for an inn the world was founded with.
    pub built: bool,
    /// Guest beds: [`INN_BEDS`].
    pub capacity: u32,
    /// One entry per guest bed, in bed order.
    pub beds: Vec<GuestBedView>,
    /// Everybody inside now, ascending by identity.
    pub occupants: Vec<SimId>,
}

/// One inn; `None` when `building` is not an inn.
pub fn inn(world: &World, building: SimId) -> Option<InnView> {
    if !is_inn(world, building) {
        return None;
    }
    let index = world.resource::<SimIndex>();
    let entity = index.get(building)?;
    let place = geometry(world, entity)?;
    let (body_lo, body_hi) = place.cells();
    let guests = occupants(world, building);
    let beds = (0..inn_beds(world))
        .map(|bed| {
            let guest = guests.iter().copied().find(|id| {
                index
                    .get(*id)
                    .and_then(|actor| world.get::<Indoors>(actor))
                    .is_some_and(|state| state.bed == bed)
            });
            let person = guest
                .and_then(|id| index.get(id))
                .and_then(|actor| world.get::<Villager>(actor));
            GuestBedView {
                index: bed,
                guest,
                name: person.map_or_else(String::new, |person| person.name.clone()),
                until: person.map_or(0, |person| person.resting_until),
            }
        })
        .collect();
    let keeper = world
        .try_query::<(Entity, &SimId, &Villager)>()
        .and_then(|mut query| {
            query
                .iter(world)
                .filter(|(actor, _, person)| {
                    person.workplace == building
                        && super::frontier::economy_spec::is_service(world, person.role)
                        && living(world, *actor)
                })
                .map(|(_, id, _)| *id)
                .min()
        });
    Some(InnView {
        inn: building,
        label: world
            .get::<VillagePlace>(entity)
            .map_or_else(String::new, |place| place.label.to_string()),
        settlement: world
            .get::<SettlementMember>(entity)
            .map(|member| member.settlement),
        keeper,
        entrance: place.entrance,
        entrance_cell: (
            place.entrance.x.div_euclid(1000),
            place.entrance.z.div_euclid(1000),
        ),
        door: place.door(),
        body_lo,
        body_hi,
        body_min: place.lo,
        body_max: place.hi,
        facing: place.facing,
        built: place.built,
        capacity: inn_beds(world),
        beds,
        occupants: guests,
    })
}

/// Every inn, ascending by identity.
pub fn inns(world: &World) -> Vec<InnView> {
    let Some(mut query) = world.try_query::<(&SimId, &VillagePlace)>() else {
        return Vec::new();
    };
    let mut ids: Vec<SimId> = query
        .iter(world)
        .filter(|(_, place)| super::frontier::economy_spec::is_service(world, place.role))
        .map(|(id, _)| *id)
        .collect();
    ids.sort_unstable();
    ids.into_iter().filter_map(|id| inn(world, id)).collect()
}

/// Whether an inn stands here: used by the rented-bed rule and by tests.
pub fn is_inn(world: &World, building: SimId) -> bool {
    world
        .resource::<SimIndex>()
        .get(building)
        .and_then(|entity| world.get::<VillagePlace>(entity))
        .is_some_and(|place| super::frontier::economy_spec::is_service(world, place.role))
}
