//! Wildlife that lives (living world only): deer graze, wander in small
//! steps inside their habitat's home range with a few metres between them,
//! gallop away from a resident who comes close and settle again.
//!
//! Every animal of a living world carries a hashed [`WildlifeMotion`]. Once a
//! tick, sorted by `SimId`, each living animal moves at most its speed toward
//! its target; timings come from `sim_rand` with this module's purposes. An
//! animal being hunted (its reservation is held) stands still, so the hunt
//! outcome is the one of standing prey. Targets are on walkable land, out of
//! water, off fields and outside buildings, and at least
//! [`SPACING_MM`] from every other animal.
use super::civic::Building;
use super::hunting::{Wildlife, WildlifeHabitat};
use super::{Villager, Vitality, living};
use bevy_app::App;
use bevy_ecs::prelude::*;
use bevy_reflect::Reflect;
use npc_framework::kernel::rng::sim_rand;
use npc_framework::prelude::*;
use serde::{Deserialize, Serialize};

/// No two animals are born or come to rest closer than this.
pub const SPACING_MM: i64 = 1_500;
/// Radius of the home range around the habitat's home.
pub const RANGE_MM: i32 = 12_000;
/// Walking and galloping speed. A resident walks at about 70 mm a tick.
pub const WALK_MM_PER_TICK: i32 = 25;
pub const GALLOP_MM_PER_TICK: i32 = 160;
/// A resident this close (and not hunting the animal) makes it flee.
pub const FLEE_REACH_MM: i64 = 8_000;
/// How far a flight goes, and how long the animal stays calm afterwards:
/// a hunter who follows catches it grazing.
const FLEE_DISTANCE_MM: i32 = 14_000;
const CALM_TICKS: u64 = 1_800;
/// Looks for danger this often.
const WATCH_TICKS: u64 = 10;
/// A moving animal takes one step of this many ticks' travel every this many
/// ticks (staggered by id): every moved position wakes the perception of the
/// residents nearby, and a step a tick cost about a tenth of the tick.
pub const STEP_TICKS: u64 = 5;
const GRAZE_PURPOSE: u32 = 0x5749_4c01;
const WANDER_X_PURPOSE: u32 = 0x5749_4c02;
const WANDER_Z_PURPOSE: u32 = 0x5749_4c03;
const WANDER_TIMEOUT_TICKS: u64 = 2_400;
const FLEE_TIMEOUT_TICKS: u64 = 600;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub enum WildlifeState {
    #[default]
    Grazing,
    Wandering,
    Fleeing,
}

/// What a living-world animal is doing and where it is going.
#[derive(
    Component, Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Reflect,
)]
#[reflect(Component)]
pub struct WildlifeMotion {
    pub state: WildlifeState,
    /// Where it walks or flees to; where it stands while grazing.
    pub target: SimPos,
    /// Tick the state ends: grazing ends, a walk or a flight gives up.
    pub until: u64,
    /// 0 while grazing.
    pub speed_mm_per_tick: i32,
    /// No flight before this tick: it has just fled and settles.
    pub calm_until: u64,
}
impl SimHash for WildlifeMotion {
    const NAME: &'static str = "regional_wildlife_motion";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.u8(self.state as u8);
        self.target.sim_hash(h);
        h.u64(self.until);
        h.i32(self.speed_mm_per_tick);
        h.u64(self.calm_until);
    }
}

pub fn register(app: &mut App) {
    app.register_sim_component::<WildlifeMotion>();
}

fn alive(world: &World, entity: Entity) -> bool {
    world
        .get::<Vitality>(entity)
        .is_some_and(|v| v.health > 0 && v.died_at.is_none())
}

/// Whether `at` is open land an animal may stand on: inside the grid, not
/// blocking (the grid holds every solid body), walkable terrain (not water),
/// not inside a civic building's or a field's footprint.
pub fn open_ground(world: &World, at: SimPos) -> bool {
    let grid = world.resource::<SpatialGrid>();
    let cell = grid.cell_of(at);
    if !grid.bounds().contains(cell)
        || grid.static_cell(cell).flags.blocking
        || super::frontier::terrain::cell(world, cell.x, cell.z)
            .is_none_or(|terrain| !terrain.walkable())
    {
        return false;
    }
    let (x, z) = (at.x.div_euclid(1_000), at.z.div_euclid(1_000));
    // No query while no building has been founded yet.
    if world.try_query::<&Building>().is_some_and(|mut buildings| {
        buildings
            .iter(world)
            .any(|building| building.plot.contains(x, z))
    }) {
        return false;
    }
    true
}

/// Positions other animals stand at or are going to, by id.
fn others(world: &World) -> Vec<(SimId, SimPos, Option<SimPos>)> {
    let mut list: Vec<_> = world
        .try_query::<(Entity, &SimId, &SimPos, &Wildlife, Option<&WildlifeMotion>)>()
        .map(|mut query| {
            query
                .iter(world)
                .filter(|(entity, ..)| alive(world, *entity))
                .map(|(_, id, at, _, motion)| (*id, *at, motion.map(|m| m.target)))
                .collect()
        })
        .unwrap_or_default();
    list.sort_unstable_by_key(|(id, ..)| *id);
    list
}

/// `at` keeps [`SPACING_MM`] from every other animal and its target.
fn spaced(others: &[(SimId, SimPos, Option<SimPos>)], me: SimId, at: SimPos) -> bool {
    others
        .iter()
        .filter(|(id, ..)| *id != me)
        .all(|(_, pos, target)| {
            pos.dist2(at) >= SPACING_MM.pow(2)
                && target.is_none_or(|target| target.dist2(at) >= SPACING_MM.pow(2))
        })
}

/// A spaced spot on open ground near `nominal`, within the home range of
/// `home`: `nominal` itself if it will do, else the nearest of a ring
/// search; `None` when there is none.
pub fn place_near(
    world: &World,
    me: SimId,
    home: SimPos,
    nominal: SimPos,
    others: &[(SimId, SimPos, Option<SimPos>)],
) -> Option<SimPos> {
    let in_range = |at: SimPos| at.dist2(home) <= i64::from(RANGE_MM).pow(2);
    let fits = |at: SimPos| in_range(at) && spaced(others, me, at) && open_ground(world, at);
    if fits(nominal) {
        return Some(nominal);
    }
    for ring in 1..=8_i32 {
        let step = ring * 1_000;
        for (dx, dz) in [
            (step, 0),
            (-step, 0),
            (0, step),
            (0, -step),
            (step, step),
            (-step, step),
            (step, -step),
            (-step, -step),
        ] {
            let at = SimPos::new(nominal.x + dx, nominal.z + dz);
            if fits(at) {
                return Some(at);
            }
        }
    }
    None
}

/// A newborn's place: the habitat's birth offset if it is spaced and open,
/// else the nearest spot that is.
pub fn birth_spot(world: &World, home: SimPos, nominal: SimPos) -> SimPos {
    if !super::frontier::enabled(world) {
        return nominal;
    }
    let others = others(world);
    place_near(world, SimId(0), home, nominal, &others).unwrap_or(nominal)
}

/// The nearest living resident within [`FLEE_REACH_MM`] of `at` who is not
/// already hunting this animal, if any.
fn threat(residents: &[(SimId, SimPos, Option<SimId>)], me: SimId, at: SimPos) -> Option<SimPos> {
    residents
        .iter()
        .filter(|(_, pos, hunting)| *hunting != Some(me) && pos.dist2(at) <= FLEE_REACH_MM.pow(2))
        .min_by_key(|(id, pos, _)| (pos.dist2(at), *id))
        .map(|(_, pos, _)| *pos)
}

/// One step of at most `speed` from `from` toward `to`.
fn step(from: SimPos, to: SimPos, speed: i32) -> SimPos {
    let (dx, dz) = (i64::from(to.x - from.x), i64::from(to.z - from.z));
    let distance = (dx * dx + dz * dz).isqrt();
    if distance <= i64::from(speed) || distance == 0 {
        return to;
    }
    let speed = i64::from(speed);
    SimPos::new(
        from.x + (dx * speed / distance) as i32,
        from.z + (dz * speed / distance) as i32,
    )
}

/// Once a tick in the living world: animals without a motion get one, then
/// every living animal, sorted by id, looks for danger, moves and changes
/// state.
pub fn tick(world: &mut World) {
    if !super::frontier::enabled(world)
        || !super::frontier::economy_spec::tuning(world).is_some_and(|t| t.wildlife_motion)
    {
        return;
    }
    let now = world.resource::<SimClock>().tick;
    let seed = world.resource::<WorldSeed>().0;
    let mut animals: Vec<(SimId, Entity, SimPos, SimId)> = world
        .query::<(Entity, &SimId, &SimPos, &Wildlife)>()
        .iter(world)
        .map(|(entity, id, at, wild)| (*id, entity, *at, wild.habitat))
        .collect();
    animals.sort_unstable_by_key(|(id, ..)| *id);
    if animals.is_empty() {
        return;
    }
    let watch = now.is_multiple_of(WATCH_TICKS);
    let residents: Vec<(SimId, SimPos, Option<SimId>)> = if watch {
        let mut list: Vec<_> = world
            .query::<(Entity, &SimId, &SimPos, &Villager)>()
            .iter(world)
            .filter(|(entity, ..)| living(world, *entity))
            .map(|(entity, id, at, _)| {
                // The animal a hunter hunts or stalks (its hunt intent) does
                // not flee from that hunter.
                let stalking = world
                    .get::<super::hunting::Hunting>(entity)
                    .map(|hunt| hunt.target)
                    .or_else(|| {
                        world
                            .get::<super::production::JobIntent>(entity)
                            .filter(|intent| intent.verb == "hunt")
                            .and_then(|intent| intent.target)
                    });
                (*id, *at, stalking)
            })
            .collect();
        list.sort_unstable_by_key(|(id, ..)| *id);
        list
    } else {
        Vec::new()
    };
    for (id, entity, at, habitat) in animals {
        if !alive(world, entity) {
            if world.get::<WildlifeMotion>(entity).is_some() {
                world.entity_mut(entity).remove::<WildlifeMotion>();
            }
            continue;
        }
        let home = world
            .resource::<SimIndex>()
            .get(habitat)
            .and_then(|e| world.get::<WildlifeHabitat>(e))
            .map_or(at, |habitat| habitat.home);
        let Some(mut motion) = world.get::<WildlifeMotion>(entity).copied() else {
            let graze = 200 + sim_rand(seed, id, now, GRAZE_PURPOSE) % 800;
            world.entity_mut(entity).insert(WildlifeMotion {
                state: WildlifeState::Grazing,
                target: at,
                until: now + graze,
                speed_mm_per_tick: 0,
                calm_until: 0,
            });
            continue;
        };
        // A hunted animal is caught: it stands while the hunt lasts.
        if world.resource::<Reservations>().count(id) > 0 {
            if motion.state != WildlifeState::Grazing {
                motion = WildlifeMotion {
                    state: WildlifeState::Grazing,
                    target: at,
                    until: now + 600,
                    speed_mm_per_tick: 0,
                    ..motion
                };
                world.entity_mut(entity).insert(motion);
            }
            continue;
        }
        if watch
            && motion.state != WildlifeState::Fleeing
            && now >= motion.calm_until
            && let Some(danger) = threat(&residents, id, at)
        {
            // Directly away from the danger, as far as the range allows.
            let (dx, dz) = (i64::from(at.x - danger.x), i64::from(at.z - danger.z));
            let distance = (dx * dx + dz * dz).isqrt().max(1);
            let away = SimPos::new(
                at.x + (dx * i64::from(FLEE_DISTANCE_MM) / distance) as i32,
                at.z + (dz * i64::from(FLEE_DISTANCE_MM) / distance) as i32,
            );
            let others = others(world);
            let target = place_near(world, id, home, away, &others)
                .or_else(|| place_near(world, id, home, home, &others))
                .unwrap_or(at);
            motion = WildlifeMotion {
                state: WildlifeState::Fleeing,
                target,
                until: now + FLEE_TIMEOUT_TICKS,
                speed_mm_per_tick: GALLOP_MM_PER_TICK,
                calm_until: now + CALM_TICKS,
            };
        }
        match motion.state {
            WildlifeState::Grazing => {
                if now >= motion.until {
                    let others = others(world);
                    let offset = |purpose| {
                        (sim_rand(seed, id, now, purpose) % (2 * RANGE_MM as u64 + 1)) as i32
                            - RANGE_MM
                    };
                    let nominal = SimPos::new(
                        home.x + offset(WANDER_X_PURPOSE) / 2,
                        home.z + offset(WANDER_Z_PURPOSE) / 2,
                    );
                    // A short walk: at most a third of the range at a time.
                    let nominal =
                        SimPos::new(at.x + (nominal.x - at.x) / 3, at.z + (nominal.z - at.z) / 3);
                    motion = match place_near(world, id, home, nominal, &others) {
                        Some(target) if target != at => WildlifeMotion {
                            state: WildlifeState::Wandering,
                            target,
                            until: now + WANDER_TIMEOUT_TICKS,
                            speed_mm_per_tick: WALK_MM_PER_TICK,
                            ..motion
                        },
                        _ => WildlifeMotion {
                            until: now + 200 + sim_rand(seed, id, now, GRAZE_PURPOSE) % 800,
                            ..motion
                        },
                    };
                }
            }
            WildlifeState::Wandering | WildlifeState::Fleeing
                if (now + u64::from(id.0)).is_multiple_of(STEP_TICKS) || now >= motion.until =>
            {
                let next = step(
                    at,
                    motion.target,
                    motion.speed_mm_per_tick * STEP_TICKS as i32,
                );
                if next != at {
                    world.entity_mut(entity).insert(next);
                }
                if next == motion.target || now >= motion.until {
                    let graze = 400 + sim_rand(seed, id, now, GRAZE_PURPOSE) % 1_200;
                    motion = WildlifeMotion {
                        state: WildlifeState::Grazing,
                        target: next,
                        until: now + graze,
                        speed_mm_per_tick: 0,
                        ..motion
                    };
                }
            }
            WildlifeState::Wandering | WildlifeState::Fleeing => {}
        }
        if world.get::<WildlifeMotion>(entity) != Some(&motion) {
            world.entity_mut(entity).insert(motion);
        }
    }
}

/// One animal as the view draws it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct WildlifeView {
    pub id: SimId,
    /// `"deer"`, or `"stag"` for every fourth animal by id.
    pub kind: &'static str,
    pub position: SimPos,
    /// Where it walks or flees to; its own position while it grazes.
    pub target: SimPos,
    /// `"grazing"`, `"walking"`, `"fleeing"` or `"dead"`.
    pub state: &'static str,
    /// 0 while grazing or dead; `WALK_MM_PER_TICK` walking,
    /// `GALLOP_MM_PER_TICK` fleeing.
    pub speed_mm_per_tick: i32,
}

/// Every wild animal, sorted by id. Without a motion (older profiles) an
/// animal reads as grazing where it stands.
pub fn views(world: &World) -> Vec<WildlifeView> {
    let Some(mut query) =
        world.try_query::<(Entity, &SimId, &SimPos, &Wildlife, Option<&WildlifeMotion>)>()
    else {
        return Vec::new();
    };
    let mut out: Vec<WildlifeView> = query
        .iter(world)
        .map(|(entity, id, at, _, motion)| {
            let dead = !alive(world, entity);
            let (state, target, speed) = match motion {
                _ if dead => ("dead", *at, 0),
                Some(motion) => (
                    match motion.state {
                        WildlifeState::Grazing => "grazing",
                        WildlifeState::Wandering => "walking",
                        WildlifeState::Fleeing => "fleeing",
                    },
                    motion.target,
                    motion.speed_mm_per_tick,
                ),
                None => ("grazing", *at, 0),
            };
            WildlifeView {
                id: *id,
                kind: if id.0 % 4 == 0 { "stag" } else { "deer" },
                position: *at,
                target,
                state,
                speed_mm_per_tick: speed,
            }
        })
        .collect();
    out.sort_unstable_by_key(|view| view.id);
    out
}
