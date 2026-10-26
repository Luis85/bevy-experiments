//! The scale scenario: thousands of wandering units, for benches, stress tests and the
//! work-counter perf gate (spec §5.7, §12.6; ADR-0010). Built in code from whatever content is
//! loaded; needs no scenario file. It runs on a flat, open map (M0's workload) or on the
//! terrain of a map with layers ([`ScaleScenario::on_terrain`]: the 2 km map [`SCALE_MAP`],
//! rivers, bridges and forests), where wandering goes through path requests.
//!
//! The scenario synthesises the real entity mix: besides the units it spawns
//! [`Follower`]s (cross-entity reads: each tracks a unit) and keeps [`Projectile`]s in flight
//! (spawn and despawn churn through the funnels), and some units run a spatial query every
//! tick. The [`ScaleMix`] resource drives the firing and the sensing; it is present only in
//! scale runs.

use bevy_ecs::component::Component;
use bevy_ecs::message::MessageWriter;
use bevy_ecs::query::Without;
use bevy_ecs::resource::Resource;
use bevy_ecs::schedule::IntoScheduleConfigs;
use bevy_ecs::system::{InMut, IntoSystem, Query, Res, ResMut};
use bevy_ecs::world::{EntityWorldMut, World};
use fl_content::{Faction, MapDef};
use fl_math::{Angle, Fx, FxVec2, Rolls, stream_key};
use fl_protocol::{Controller, FactionId, PlayerId, SimId, TeamId, UnitTypeId};
use serde::{Deserialize, Serialize};

use crate::build::Content;
use crate::builder::SimBuilder;
use crate::classify::Class;
use crate::clock::SimClock;
use crate::counters::SimCounters;
use crate::hash::{SimHash, StateHasher};
use crate::ids::{SimEntity, SimIndex};
use crate::map::MapInfo;
use crate::movement::move_system;
use crate::nav::{MoveProfile, NavGrid, cell_centre, def_profile, nearest_passable};
use crate::players::{PlayerInfo, Players};
use crate::rng::{MatchSeed, Stream, rolls};
use crate::scenario::SetupError;
use crate::schedule::{ActSet, PerceiveSet, ResolveSet};
use crate::spatial::{SpatialGrid, init_spatial};
use crate::spawn::{DespawnRequest, SpawnError, SpawnRequest, setup_spawn};
use crate::terrain::init_terrain;
use crate::units::{MobilityDef, Owner, Pose, Wander};

/// The key of the `"setup.scale"` stream, which places the units and the followers.
const PLACEMENT_STREAM: u64 = stream_key("setup.scale");

/// Cell side of the flat scale map, in metres.
const CELL_M: i32 = 4;

/// The map of [`ScaleScenario::on_terrain`]: 2 km square, 4 m cells, with layers.
pub const SCALE_MAP: &str = "base.scale_2km";

/// How many times a unit's position is redrawn when its cell is not passable for its
/// profile, before the nearest passable cell is taken.
const PLACEMENT_REDRAWS: u32 = 16;

/// A follower's offset from its unit: uniform in `±FOLLOW_OFFSET_M` on each axis, in metres.
const FOLLOW_OFFSET_M: i32 = 10;

/// How far a projectile flies per tick, in metres (200 m/s at 20 Hz).
const PROJECTILE_SPEED: Fx = Fx::from_int(10);

/// The radius of a sensing unit's spatial query, in metres.
const SENSOR_RADIUS: Fx = Fx::from_int(150);

/// `units` wandering units for `players` players on a `map_m` × `map_m` map (flat and open, or
/// the terrain of map `map`), with `followers_per_unit` followers each and about
/// `projectiles` projectiles in flight.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScaleScenario {
    pub units: u32,
    pub players: u8,
    /// Map side in metres.
    pub map_m: i32,
    /// Wander radius in metres.
    pub wander_m: i32,
    /// The scenario's seed: the match seed the sim must be built with; it places the units.
    pub seed: u64,
    /// Followers spawned for every unit.
    pub followers_per_unit: u32,
    /// Projectiles in flight once the first ones expire: `ceil(projectiles / projectile_ttl)`
    /// are fired every tick.
    pub projectiles: u32,
    /// Ticks a projectile flies before it is despawned; at least 1 when `projectiles` is not
    /// 0.
    pub projectile_ttl: u32,
    /// A unit runs a sensor query every `sensor_every` ticks; 0 turns sensing off.
    pub sensor_every: u32,
    /// The map whose terrain the units stand on (its side must be `map_m`); `None`: a flat,
    /// open map (M0's workload).
    pub map: Option<String>,
}

impl Default for ScaleScenario {
    fn default() -> Self {
        ScaleScenario {
            units: 3000,
            players: 4,
            map_m: 2000,
            wander_m: 150,
            seed: 1,
            followers_per_unit: 2,
            projectiles: 1000,
            projectile_ttl: 20,
            sensor_every: 10,
            map: None,
        }
    }
}

/// A follower: every tick, after movement, it stands at `offset` from its `leader`, clamped
/// to the map. Hashed `"core.follower"`.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Follower {
    pub leader: SimId,
    pub offset: FxVec2,
}

/// A projectile in flight: it moves `vel` metres per tick, clamped to the map, and is
/// despawned when `ttl` reaches 0. A stand-in until weapons arrive (M2). Hashed
/// `"core.projectile"`.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Projectile {
    pub vel: FxVec2,
    pub ttl: u16,
}

/// What a scale run fires and senses with; inserted by [`ScaleScenario::populate`], absent
/// otherwise. Hashed `"core.scale_mix"`: it drives the firing, so saves, branches and replays
/// of scale runs carry it.
#[derive(Resource, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScaleMix {
    /// Projectiles in flight in the steady state.
    projectiles: u32,
    /// The flight time of a new projectile, in ticks; at least 1.
    projectile_ttl: u16,
    /// 0: no sensing.
    sensor_every: u32,
    /// The units, ascending: the shooters and the sensors.
    units: Vec<SimId>,
}

impl SimHash for ScaleMix {
    fn sim_hash(&self, h: &mut StateHasher) {
        self.projectiles.sim_hash(h);
        self.projectile_ttl.sim_hash(h);
        self.sensor_every.sim_hash(h);
        let count = u32::try_from(self.units.len()).expect("fewer than u32::MAX units");
        count.sim_hash(h);
        for id in &self.units {
            id.sim_hash(h);
        }
    }
}

impl SimHash for Follower {
    fn sim_hash(&self, h: &mut StateHasher) {
        self.leader.sim_hash(h);
        self.offset.sim_hash(h);
    }
}

impl SimHash for Projectile {
    fn sim_hash(&self, h: &mut StateHasher) {
        self.vel.sim_hash(h);
        self.ttl.sim_hash(h);
    }
}

impl ScaleScenario {
    /// The version of [`ScaleScenario::encode`]'s params, recorded in a replay's scale start
    /// (ADR-0021, M10): a new field bumps it, and [`ScaleScenario::decode`] keeps reading the
    /// older versions, so the replay format does not change.
    /// Version 1: the fields at tag `m1` (through `map`).
    pub const VERSION: u32 = 1;

    /// The scenario as versioned params: ([`ScaleScenario::VERSION`], its postcard encoding).
    ///
    /// # Panics
    /// Never: every field serializes.
    pub fn encode(&self) -> (u32, Vec<u8>) {
        let params = postcard::to_allocvec(self).expect("scale scenarios serialize");
        (Self::VERSION, params)
    }

    /// The scenario that [`ScaleScenario::encode`] wrote as `bytes` at `version`.
    ///
    /// # Errors
    /// A message naming the version if this build does not read it ("ScaleScenario version
    /// 2 is not supported ..."), or the decode error, trailing bytes included.
    pub fn decode(version: u32, bytes: &[u8]) -> Result<ScaleScenario, String> {
        if version != Self::VERSION {
            return Err(format!(
                "ScaleScenario version {version} is not supported (this build reads {})",
                Self::VERSION
            ));
        }
        match postcard::take_from_bytes::<ScaleScenario>(bytes) {
            Ok((scale, [])) => Ok(scale),
            Ok((_, rest)) => Err(format!(
                "ScaleScenario version {version}: {} trailing bytes",
                rest.len()
            )),
            Err(err) => Err(format!("ScaleScenario version {version}: {err}")),
        }
    }

    /// The default mix scaled to `units` units: 2 followers per unit and `units / 3`
    /// projectiles in flight.
    pub fn with_units(units: u32) -> Self {
        ScaleScenario {
            units,
            projectiles: units / 3,
            ..ScaleScenario::default()
        }
    }

    /// [`ScaleScenario::with_units`] on the terrain of [`SCALE_MAP`] (2 km square, as
    /// `map_m`).
    pub fn on_terrain(units: u32) -> Self {
        ScaleScenario {
            map: Some(SCALE_MAP.to_owned()),
            ..ScaleScenario::with_units(units)
        }
    }

    /// Set up the match, spawn the units and their followers through [`setup_spawn`] and
    /// insert the [`ScaleMix`]; returns the units' ids in spawn order (ascending).
    ///
    /// - Map: without `map`, flat, `map_m` square, 4 m cells, open ground; with `map`, that
    ///   map's size, cells and terrain (its layers; open ground if it has none).
    /// - Player `i`: faction `i % factions` (factions in id order), team `i`, no controller.
    /// - Unit `k`: player `k % players`, unit type `k % types` over the concrete units with
    ///   `Mobility` (in id order), heading 0, at a uniform point in quadrant `player % 4`
    ///   (0 south-west, 1 south-east, 2 north-west, 3 north-east), drawn x then y from
    ///   `Rolls::new(seed, 0, 0, stream_key("setup.scale"))`. A point whose cell the unit's
    ///   movement profile cannot stand on is redrawn, x then y from the same draws, up to 16
    ///   times; if the last is still not passable, the unit stands at the centre of the
    ///   nearest passable cell ([`nearest_passable`]). Every unit gets
    ///   `Wander { home: pos, radius: wander_m }`.
    /// - Then, unit by unit in id order, its `followers_per_unit` followers: non-unit
    ///   entities of the unit's player, each with an offset drawn x then y, uniform in
    ///   ±10 m, from the same draws (after every unit's), standing at the unit's position
    ///   plus the offset, clamped to the map, with a [`Follower`].
    ///
    /// Each tick `ceil(projectiles / projectile_ttl)` units, drawn from
    /// `rolls(seed, tick, SimId(0), stream_key("scale.fire"))`, fire a [`Projectile`] from
    /// their position, at an angle drawn after the shooter, 10 m per tick, for
    /// `projectile_ttl` ticks; and every unit whose `sim_id % sensor_every` is
    /// `tick % sensor_every` queries the [`SpatialGrid`] within 150 m.
    ///
    /// `seed` is the scenario's seed, as a scenario file's is: the sim must have been built
    /// with it (`MatchSetup { seed, .. }`), so the [`MatchSeed`] the sim draws from is the
    /// seed its `MatchHeader` records.
    ///
    /// # Errors
    /// [`SetupError::SeedMismatch`] if the [`MatchSeed`] is not `seed`,
    /// [`SetupError::UnknownMap`] if `map` is not loaded, [`SetupError::MapSizeMismatch`] if
    /// it is not `map_m` square, [`SetupError::UnknownFaction`] if no faction is loaded, and
    /// [`SpawnError::UnknownUnit`] if no concrete unit has `Mobility`; the world is unchanged.
    ///
    /// # Panics
    /// If `players` is 0 while `units` is not, `map_m` is not positive, or `projectiles` is
    /// not 0 while `projectile_ttl` is not in `1..=65535`.
    pub fn populate(&self, world: &mut World) -> Result<Vec<SimId>, SetupError> {
        let matched = world.resource::<MatchSeed>().0;
        if matched != self.seed {
            return Err(SetupError::SeedMismatch {
                scale: self.seed,
                matched,
            });
        }
        let content = world.resource::<Content>().0.clone();
        let map_def = match &self.map {
            Some(id) => {
                let def = content
                    .kind::<MapDef>()
                    .get(id)
                    .ok_or_else(|| SetupError::UnknownMap(id.clone()))?;
                let side = Fx::from_int(self.map_m);
                if def.size_m != FxVec2::new(side, side) {
                    return Err(SetupError::MapSizeMismatch {
                        map: id.clone(),
                        map_m: self.map_m,
                    });
                }
                Some(def)
            }
            None => None,
        };
        let faction_count = content.kind::<Faction>().len();
        if faction_count == 0 {
            return Err(SetupError::UnknownFaction(
                "(no factions loaded)".to_owned(),
            ));
        }
        let types: Vec<UnitTypeId> = content
            .units()
            .iter()
            .enumerate()
            .filter(|(_, unit)| unit.get::<MobilityDef>(content.registry()).is_some())
            .map(|(i, _)| UnitTypeId(u16::try_from(i).expect("loading caps units at u16::MAX")))
            .collect();
        if types.is_empty() {
            return Err(SpawnError::UnknownUnit.into());
        }
        assert!(
            self.players > 0 || self.units == 0,
            "a scale scenario with units needs players"
        );
        let projectile_ttl = match u16::try_from(self.projectile_ttl) {
            Ok(ttl) if ttl > 0 => ttl,
            _ if self.projectiles == 0 => 1,
            _ => panic!(
                "projectile_ttl {} must be in 1..=65535 when projectiles are fired",
                self.projectile_ttl
            ),
        };

        let map = match map_def {
            Some(def) => MapInfo::flat(def.size_m, def.cell_m),
            None => {
                let side = Fx::from_int(self.map_m);
                MapInfo::flat(FxVec2::new(side, side), Fx::from_int(CELL_M))
            }
        };
        world.insert_resource(map.clone());
        init_spatial(world);
        init_terrain(world, map_def);
        let players = (0..self.players)
            .map(|i| PlayerInfo {
                id: PlayerId(i),
                faction: FactionId(
                    u16::try_from(usize::from(i) % faction_count).expect("fewer than 256"),
                ),
                team: TeamId(i),
                controller: Controller::None,
            })
            .collect();
        world.insert_resource(Players { list: players });

        let half = Fx::from_ratio(i64::from(self.map_m), 2);
        let radius = Fx::from_int(self.wander_m);
        #[expect(
            clippy::disallowed_methods,
            reason = "setup code: placement draws use the scenario's own seed, not the match seed"
        )]
        let mut placement = Rolls::new(self.seed, 0, 0, PLACEMENT_STREAM);
        // Every unit's player, type and position, drawn before anything spawns (the draws
        // borrow the navigation grid).
        let placed: Vec<(u8, UnitTypeId, FxVec2)> = {
            let nav = world.get_resource::<NavGrid>();
            let profiles: Vec<Option<MoveProfile>> = types
                .iter()
                .map(|&unit| {
                    let nav = nav?;
                    let def = content.unit(unit)?;
                    def_profile(&content, def, nav.cell_m(), nav.max_clearance())
                })
                .collect();
            (0..self.units)
                .map(|k| {
                    let player =
                        u8::try_from(k % u32::from(self.players)).expect("player < u8::MAX");
                    let quadrant = player % 4;
                    let x0 = half.mul_int(i32::from(quadrant & 1));
                    let y0 = half.mul_int(i32::from(quadrant >> 1));
                    let mut draw = || {
                        let x = placement.range_fx(x0, x0 + half);
                        let y = placement.range_fx(y0, y0 + half);
                        FxVec2::new(x, y)
                    };
                    let slot = k as usize % types.len();
                    let pos = match (nav, profiles[slot]) {
                        (Some(nav), Some(profile)) => place(nav, profile, &mut draw),
                        _ => draw(),
                    };
                    (player, types[slot], pos)
                })
                .collect()
        };
        let mut ids = Vec::with_capacity(self.units as usize);
        let mut homes = Vec::with_capacity(self.units as usize);
        for &(player, unit, pos) in &placed {
            let request = SpawnRequest {
                unit: Some(unit),
                owner: PlayerId(player),
                pos,
                heading: Angle::ZERO,
                init: None,
            };
            let id = setup_spawn(world, &request)?;
            let entity = world.resource::<SimIndex>().get(id).expect("just spawned");
            world
                .entity_mut(entity)
                .insert(Wander { home: pos, radius });
            ids.push(id);
            homes.push((PlayerId(player), pos));
        }

        let reach = Fx::from_int(FOLLOW_OFFSET_M);
        for (&leader, &(owner, home)) in ids.iter().zip(&homes) {
            for _ in 0..self.followers_per_unit {
                let x = placement.range_fx(-reach, reach);
                let y = placement.range_fx(-reach, reach);
                let offset = FxVec2::new(x, y);
                let request = SpawnRequest {
                    unit: None,
                    owner,
                    pos: map.clamp(home + offset),
                    heading: Angle::ZERO,
                    init: None,
                };
                let id = setup_spawn(world, &request)?;
                let entity = world.resource::<SimIndex>().get(id).expect("just spawned");
                world.entity_mut(entity).insert(Follower { leader, offset });
            }
        }

        world.insert_resource(ScaleMix {
            projectiles: self.projectiles,
            projectile_ttl,
            sensor_every: self.sensor_every,
            units: ids.clone(),
        });
        Ok(ids)
    }
}

/// A position from `draw` whose cell `profile` can stand on (see
/// [`ScaleScenario::populate`]): the first draw, else one of up to [`PLACEMENT_REDRAWS`]
/// more, else the centre of the passable cell nearest the last one (the last draw itself if
/// no cell of the map is passable: its spawn then fails).
fn place(nav: &NavGrid, profile: MoveProfile, draw: &mut impl FnMut() -> FxVec2) -> FxVec2 {
    let passable = |pos: FxVec2| {
        let (x, y) = nav.cell_of(pos);
        nav.passable(profile, i64::from(x), i64::from(y))
    };
    let mut pos = draw();
    for _ in 0..PLACEMENT_REDRAWS {
        if passable(pos) {
            return pos;
        }
        pos = draw();
    }
    if passable(pos) {
        return pos;
    }
    let (cells_x, cells_y) = nav.cells();
    let mut probed = 0;
    nearest_passable(
        nav,
        profile,
        nav.cell_of(pos),
        cells_x.max(cells_y),
        &mut probed,
    )
    .map_or(pos, |cell| cell_centre(nav, cell))
}

/// Put every [`Follower`] at its leader's position plus its offset, clamped to the map; a
/// follower whose leader is gone stays put. Leaders are never followers, so this reads only
/// positions [`move_system`] has finished writing. Counts `"scale.follows"`.
pub(crate) fn follow_system(
    map: Option<Res<MapInfo>>,
    index: Res<SimIndex>,
    leaders: Query<&Pose, Without<Follower>>,
    mut followers: Query<(&Follower, &mut Pose)>,
    mut counters: ResMut<SimCounters>,
) {
    let Some(map) = map else {
        return;
    };
    let mut follows = 0;
    for (follower, mut pose) in &mut followers {
        let Some(leader) = index
            .get(follower.leader)
            .and_then(|entity| leaders.get(entity).ok())
        else {
            continue;
        };
        pose.pos = map.clamp(leader.pos + follower.offset);
        follows += 1;
    }
    if follows > 0 {
        counters.add("scale.follows", follows);
    }
}

/// The [`SpawnInit`](crate::SpawnInit) of a projectile: a [`Projectile`] flying along the
/// request's heading at 10 m per tick, for the [`ScaleMix`]'s flight time.
fn init_projectile(entity: &mut EntityWorldMut<'_>, req: &SpawnRequest) {
    let ttl = entity
        .world()
        .get_resource::<ScaleMix>()
        .map_or(1, |mix| mix.projectile_ttl);
    entity.insert(Projectile {
        vel: FxVec2::from_angle(req.heading) * PROJECTILE_SPEED,
        ttl,
    });
}

/// Fire this tick's `ceil(projectiles / projectile_ttl)` projectiles: for each, draw the
/// shooter (an index into the units), then the angle, from the world-level `"scale.fire"`
/// draws of the tick, and request the spawn at the shooter's position. Only with a
/// [`ScaleMix`].
pub(crate) fn scale_fire(
    InMut(stream): InMut<Stream>,
    mix: Option<Res<ScaleMix>>,
    clock: Res<SimClock>,
    seed: Res<MatchSeed>,
    index: Res<SimIndex>,
    units: Query<(&Owner, &Pose)>,
    mut spawns: MessageWriter<SpawnRequest>,
) {
    let Some(mix) = mix else {
        return;
    };
    if mix.projectiles == 0 || mix.units.is_empty() {
        return;
    }
    let shots = mix.projectiles.div_ceil(u32::from(mix.projectile_ttl));
    let count = u32::try_from(mix.units.len()).expect("fewer than u32::MAX units");
    let mut draws = rolls(&seed, clock.tick, SimId(0), *stream);
    for _ in 0..shots {
        let shooter = mix.units[draws.range_u32(0, count) as usize];
        let heading =
            Angle::from_raw(u16::try_from(draws.range_u32(0, 1 << 16)).expect("drawn below 2^16"));
        let Some((owner, pose)) = index.get(shooter).and_then(|entity| units.get(entity).ok())
        else {
            continue;
        };
        spawns.write(SpawnRequest {
            unit: None,
            owner: owner.0,
            pos: pose.pos,
            heading,
            init: Some(init_projectile),
        });
    }
}

/// Move every [`Projectile`] by its velocity, clamped to the map, and count its flight
/// time down; at 0, request its despawn. Counts `"scale.projectile_steps"`.
pub(crate) fn projectile_step(
    map: Option<Res<MapInfo>>,
    mut projectiles: Query<(&SimEntity, &mut Pose, &mut Projectile)>,
    mut despawns: MessageWriter<DespawnRequest>,
    mut counters: ResMut<SimCounters>,
) {
    let Some(map) = map else {
        return;
    };
    let mut steps = 0;
    for (sim_entity, mut pose, mut projectile) in &mut projectiles {
        pose.pos = map.clamp(pose.pos + projectile.vel);
        projectile.ttl = projectile.ttl.saturating_sub(1);
        if projectile.ttl == 0 {
            despawns.write(DespawnRequest(sim_entity.id()));
        }
        steps += 1;
    }
    if steps > 0 {
        counters.add("scale.projectile_steps", steps);
    }
}

/// The sensor stand-in: every unit whose `sim_id % sensor_every` is `tick % sensor_every`
/// queries the [`SpatialGrid`] within 150 m of itself. Observe-only: the results are
/// dropped. Counts `"spatial.queries"` and `"spatial.query_candidates"`. Only with a
/// [`ScaleMix`] whose `sensor_every` is not 0.
pub(crate) fn scale_sense(
    mix: Option<Res<ScaleMix>>,
    clock: Res<SimClock>,
    grid: Option<Res<SpatialGrid>>,
    index: Res<SimIndex>,
    poses: Query<&Pose>,
    mut counters: ResMut<SimCounters>,
) {
    let (Some(mix), Some(grid)) = (mix, grid) else {
        return;
    };
    let every = mix.sensor_every;
    if every == 0 {
        return;
    }
    let phase = clock.tick.0 % every;
    let mut found = Vec::new();
    let (mut queries, mut candidates) = (0, 0);
    for &id in mix.units.iter().filter(|id| id.0 % every == phase) {
        let Some(pose) = index.get(id).and_then(|entity| poses.get(entity).ok()) else {
            continue;
        };
        candidates += u64::from(grid.query_radius(pose.pos, SENSOR_RADIUS, &mut found));
        queries += 1;
    }
    if queries > 0 {
        counters.add("spatial.queries", queries);
    }
    if candidates > 0 {
        counters.add("spatial.query_candidates", candidates);
    }
}

/// The scale mix's components, resource, stream and systems: following after movement in
/// [`ActSet::Move`], firing in [`ActSet::Fire`], projectiles in
/// [`ResolveSet::Projectiles`] and sensing in [`PerceiveSet::Vision`].
pub(crate) fn build(sim: &mut SimBuilder) {
    sim.register_component::<Follower>("core.follower", Class::hashed(1))
        .register_component::<Projectile>("core.projectile", Class::hashed(1))
        .register_resource::<ScaleMix>("core.scale_mix", Class::hashed(1));
    let fire = sim.register_stream("scale.fire");
    sim.add_systems(ActSet::Move, follow_system.after(move_system))
        .add_systems(ActSet::Fire, scale_fire.with_input(fire))
        .add_systems(ResolveSet::Projectiles, projectile_step)
        .add_systems(PerceiveSet::Vision, scale_sense);
}
