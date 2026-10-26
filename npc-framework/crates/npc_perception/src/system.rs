//! The perception pass (`NpcSet::Sense`) and the facing update
//! (`NpcSet::Cleanup`).

use bevy_ecs::message::Messages;
use bevy_ecs::prelude::*;
use npc_ai::{BbKey, BbValue, Blackboard};
use npc_kernel::inspect;
use npc_kernel::journal::{self, JournalKind};
use npc_kernel::rng::WorldSeed;
use npc_kernel::spatial::SpatialGrid;
use npc_kernel::{PrevSimPos, SimClock, SimId, SimPos};
use tracing::info_span;

use crate::PerceptionConfig;
use crate::camera::{self, CameraHit};
use crate::detect::{Detection, DetectionCheckFn, DetectionChecks, NoiseIndex};
use crate::lidar::{LidarInput, LidarObstacles, LidarScratch, scan_lidar};
use crate::memory::{
    CONFIDENCE_FULL, HeardNoise, Percept, PerceptionFacts, PerceptionMemory, SenseKind,
};
use crate::noise::Noise;
use crate::radar::{self, RadarHit};
use crate::readings::{SensorReadings, bearing_mdeg};
use crate::sensors::{Facing, FixedFacing, Perceivable, ReachSense, Sensors};

/// One agent, as gathered at the start of the pass.
struct Agent {
    id: SimId,
    entity: Entity,
    pos: SimPos,
    /// The largest hearing radius (`None`: deaf). The full [`Sensors`] are
    /// read from the world only for the agents that scan this tick.
    hearing: Option<i32>,
    facing: Facing,
    /// It has a LIDAR, camera or radar (keeps [`SensorReadings`]).
    robot: bool,
}

/// Reused buffers of the robot sensors (plan Task 12): not state, kept
/// across ticks so a warm scan allocates nothing.
#[derive(Default)]
pub struct RobotScratch {
    /// The scanning robots' [`SensorReadings`], taken out of the world for
    /// the read-only part of the pass: `(index in the sorted agents, readings)`.
    taken: Vec<(usize, SensorReadings)>,
    work: RobotWork,
}

#[derive(Default)]
struct RobotWork {
    lidar: LidarScratch,
    camera_hits: Vec<CameraHit>,
    radar_hits: Vec<RadarHit>,
    ids: Vec<SimId>,
}

/// A detection that passed the check.
#[derive(Clone, Copy)]
struct Sensed {
    via: SenseKind,
    pos: SimPos,
    kinds: u32,
    confidence: u16,
}

/// What the pass decided for one agent (applied after all are computed).
#[derive(Default)]
struct Outcome {
    /// Heard noises (sorted), with the source's percept if it passed the check.
    heard: Vec<(Noise, Option<(SimId, Sensed)>)>,
    /// `Some` on the agent's stagger tick: everything sensed by vision,
    /// proximity or reach, ascending `SimId`.
    scan: Option<Vec<(SimId, Sensed)>>,
}

/// Whether `id` runs its vision/proximity scan, forgetting and facts in
/// `tick`: `(sim_id + tick) % stagger == 0` (`stagger` 0 counts as 1).
pub fn is_due(id: SimId, tick: u64, stagger: u32) -> bool {
    (u64::from(id.0) + tick).is_multiple_of(u64::from(stagger.max(1)))
}

/// Whether `id`'s LIDAR scans in `tick`: half a stagger after its
/// perception scan ([`is_due`] shifted by `stagger / 2`), so the LIDAR and
/// the camera/radar pass of one robot never share a tick (with `stagger`
/// 1 both run every tick).
pub fn is_lidar_due(id: SimId, tick: u64, stagger: u32) -> bool {
    is_due(id, tick + u64::from(stagger.max(1) / 2), stagger)
}

/// Line of sight from `a` to `b`: no occluding cell strictly between their
/// cells (the observer's and the target's own cells never block).
pub fn line_of_sight(grid: &SpatialGrid, a: SimPos, b: SimPos) -> bool {
    let (start, end) = (grid.cell_of(a), grid.cell_of(b));
    grid.trace_segment(a, b, |c| c == start || c == end || !grid.is_occluding(c))
}

/// The sensing agents ([`perception_system`]'s cached query: a `QueryState`
/// built every tick costs more than a quiet tick's whole pass).
pub type AgentQuery = QueryState<(
    Entity,
    &'static SimId,
    &'static SimPos,
    &'static Sensors,
    &'static Facing,
)>;

/// The perceivable entities ([`perception_system`]'s cached query).
pub type PerceivableQuery = QueryState<(
    Entity,
    &'static SimId,
    &'static SimPos,
    &'static Perceivable,
)>;

/// `NpcSet::Sense` (exclusive), in [`NpcPerceptionSet`](crate::NpcPerceptionSet):
///
/// 1. drains every pending [`Noise`] and sorts them;
/// 2. **hearing**, every tick, every agent with hearing: each noise within
///    both radii is heard (the source, if any, becomes a hearing percept);
/// 3. **scan**, only on the agent's stagger tick ([`is_due`]): every
///    `Perceivable` entity in a vision cone (range, integer FOV, line of
///    sight over occluding cells), within proximity, or within the agent's
///    [`ReachSense`] radius with line of sight (any direction);
/// 4. every candidate goes through the active detection check;
/// 5. memory updates per agent in `SimId` order, with journal `Perceived`
///    (newly sensed) and `Lost` (no longer sensed / forgotten), then the
///    blackboard facts.
///
/// Robots (plan Task 12): on its scan tick an agent's LIDAR traces its
/// beams, and its camera and radar sense what is in their cones with line
/// of sight, automatically (no detection check); camera and radar feed the
/// memory like vision (`camera`, `radar`), the LIDAR feeds none. Their
/// ROS-shaped output lands in the agent's [`SensorReadings`].
pub fn perception_system(
    world: &mut World,
    agents_q: &mut AgentQuery,
    things_q: &mut PerceivableQuery,
    mut scratch: Local<RobotScratch>,
) {
    let _span = info_span!("mechanic", name = "perception").entered();
    let tick = world.get_resource::<SimClock>().map_or(0, |c| c.tick);
    let config = world
        .get_resource::<PerceptionConfig>()
        .copied()
        .unwrap_or_default();
    let mut noises: Vec<Noise> = world
        .get_resource_mut::<Messages<Noise>>()
        .map(|mut m| m.drain().collect())
        .unwrap_or_default();
    noises.sort_by_key(Noise::sort_key);

    let mut agents: Vec<Agent> = agents_q
        .iter(world)
        .map(|(entity, &id, &pos, sensors, &facing)| Agent {
            id,
            entity,
            pos,
            hearing: sensors.hearing_radius(),
            facing,
            robot: sensors.has_robot_sensors(),
        })
        .collect();
    if agents.is_empty() {
        return;
    }
    agents.sort_unstable_by_key(|a| a.id);
    // The perceivables are read by the scans and by heard noises only: on a
    // tick where no agent scans and nothing was heard (most ticks with
    // `stagger` > 1), skip gathering and sorting every one of them.
    let lidar_due = |a: &Agent| a.robot && is_lidar_due(a.id, tick, config.stagger);
    let needed = !noises.is_empty()
        || agents
            .iter()
            .any(|a| is_due(a.id, tick, config.stagger) || lidar_due(a));
    let mut things: Vec<Thing> = if needed {
        things_q
            .iter(world)
            .map(|(entity, &id, &pos, p)| Thing {
                id,
                entity,
                pos,
                kinds: p.kinds,
            })
            .collect()
    } else {
        Vec::new()
    };
    things.sort_unstable_by_key(|t| t.id);
    if !world.contains_resource::<SpatialGrid>() {
        return;
    }
    let seed = world.get_resource::<WorldSeed>().map_or(0, |s| s.0);
    // The scanning robots' readings, taken out (buffers and all) so the
    // read-only part below can fill them in place.
    let RobotScratch { taken, work } = &mut *scratch;
    taken.clear();
    for (i, a) in agents.iter().enumerate() {
        if a.robot && (is_due(a.id, tick, config.stagger) || lidar_due(a)) {
            let mut e = world.entity_mut(a.entity);
            let readings = match e.get_mut::<SensorReadings>() {
                Some(mut r) => std::mem::take(&mut *r),
                None => {
                    e.insert(SensorReadings::default());
                    SensorReadings::default()
                }
            };
            taken.push((i, readings));
        }
    }

    let outcomes = {
        let world: &World = world;
        // Checked before the readings were taken out, so they always go back.
        let grid = world
            .get_resource::<SpatialGrid>()
            .expect("the grid was checked above");
        let check = world
            .get_resource::<DetectionChecks>()
            .map_or(crate::detect::automatic as DetectionCheckFn, |c| {
                c.active_fn()
            });
        let ctx = Ctx {
            world,
            grid,
            check,
            tick,
            seed,
            obstacles: world
                .get_resource::<LidarObstacles>()
                .copied()
                .unwrap_or_default()
                .kinds,
            things: &things,
        };
        let mut outcomes: Vec<Outcome> = agents.iter().map(|_| Outcome::default()).collect();
        hear(&ctx, &agents, &noises, &mut outcomes);
        let mut robots = taken.iter_mut().peekable();
        for (i, (agent, outcome)) in agents.iter().zip(outcomes.iter_mut()).enumerate() {
            let due = is_due(agent.id, tick, config.stagger);
            if !due && !lidar_due(agent) {
                continue;
            }
            // Every agent came from the query of `Sensors` holders.
            let sensors = world
                .get::<Sensors>(agent.entity)
                .expect("an agent has Sensors");
            // Taken above exactly for the robots due now, in agent order.
            let mut robot = robots.next_if(|(j, _)| *j == i).map(|(_, r)| r);
            if lidar_due(agent)
                && let Some(readings) = robot.as_deref_mut()
            {
                run_lidar(&ctx, agent, sensors, readings, work);
            }
            if due {
                let reach = world.get::<ReachSense>(agent.entity).copied();
                outcome.scan = Some(scan(
                    &ctx,
                    agent,
                    sensors,
                    reach,
                    config.memory_capacity as usize,
                    robot.map(|r| (r, &mut *work)),
                ));
            }
        }
        debug_assert!(robots.next().is_none(), "every taken reading was used");
        outcomes
    };
    for (i, readings) in taken.drain(..) {
        if let Some(mut r) = world.get_mut::<SensorReadings>(agents[i].entity) {
            *r = readings;
        }
    }

    let recording = journal::is_recording(world);
    let mut entries: Vec<(SimId, JournalKind)> = Vec::new();
    for (agent, outcome) in agents.iter().zip(outcomes) {
        {
            let Some(mut memory) = world.get_mut::<PerceptionMemory>(agent.entity) else {
                continue;
            };
            let mut log = |kind: JournalKind| {
                if recording {
                    entries.push((agent.id, kind));
                }
            };
            apply(&mut memory, &config, tick, outcome, &mut log);
        }
        let facts = world.get::<PerceptionFacts>(agent.entity).copied();
        if let Some(facts) = facts
            && is_due(agent.id, tick, config.stagger)
        {
            write_facts(world, agent, facts);
        }
    }
    for (id, kind) in entries {
        journal::record(world, Some(id), || kind);
    }
}

struct Ctx<'w> {
    world: &'w World,
    grid: &'w SpatialGrid,
    check: DetectionCheckFn,
    tick: u64,
    /// The world seed (LIDAR noise).
    seed: u64,
    /// [`LidarObstacles::kinds`].
    obstacles: u32,
    /// Every `Perceivable` sim entity, ascending `SimId`.
    things: &'w [Thing],
}

/// A perceivable entity, as gathered at the start of the pass.
#[derive(Clone, Copy)]
struct Thing {
    id: SimId,
    entity: Entity,
    pos: SimPos,
    kinds: u32,
}

impl Ctx<'_> {
    fn thing(&self, id: SimId) -> Option<&Thing> {
        self.things
            .binary_search_by_key(&id, |t| t.id)
            .ok()
            .map(|i| &self.things[i])
    }
}

impl Ctx<'_> {
    fn detect(
        &self,
        agent: &Agent,
        target: Option<(SimId, Entity)>,
        via: SenseKind,
        dist2: i64,
        noise: Option<NoiseIndex>,
    ) -> Option<u16> {
        (self.check)(
            self.world,
            &Detection {
                observer: agent.entity,
                observer_id: agent.id,
                target: target.map(|t| t.1),
                target_id: target.map(|t| t.0),
                via,
                dist2,
                tick: self.tick,
                noise,
            },
        )
    }
}

fn hear(ctx: &Ctx, agents: &[Agent], noises: &[Noise], outcomes: &mut [Outcome]) {
    if noises.is_empty() {
        return;
    }
    // `agents` is sorted by `SimId`: listeners are found by binary search.
    if agents.iter().all(|a| a.hearing.is_none()) {
        return;
    }
    let mut of_source = 0u32;
    for (ordinal, noise) in noises.iter().enumerate() {
        of_source = match ordinal {
            0 => 0,
            _ if noises[ordinal - 1].source == noise.source => of_source + 1,
            _ => 0,
        };
        let index = NoiseIndex {
            ordinal: u32::try_from(ordinal).unwrap_or(u32::MAX),
            of_source,
        };
        let source = noise
            .source
            .and_then(|s| ctx.grid.entity_of(s).map(|e| (s, e)));
        for id in ctx.grid.in_radius(noise.pos, noise.radius_mm) {
            let Ok(i) = agents.binary_search_by_key(&id, |a| a.id) else {
                continue;
            };
            if agents[i].hearing.is_none() {
                continue;
            }
            if noise.source == Some(id) {
                continue;
            }
            let agent = &agents[i];
            let radius = i64::from(agent.hearing.unwrap_or(0));
            let d2 = agent.pos.dist2(noise.pos);
            if d2 > radius * radius {
                continue;
            }
            let Some(confidence) = ctx.detect(agent, source, SenseKind::Hearing, d2, Some(index))
            else {
                continue;
            };
            let percept = source.map(|(sid, _)| {
                let kinds = ctx.thing(sid).map_or(0, |t| t.kinds);
                (
                    sid,
                    Sensed {
                        via: SenseKind::Hearing,
                        pos: noise.pos,
                        kinds,
                        confidence,
                    },
                )
            });
            outcomes[i].heard.push((*noise, percept));
        }
    }
}

/// Vision/proximity/reach scan. Candidates are visited nearest first (ties:
/// lower `SimId`) and the scan stops once `limit` (the memory capacity)
/// things are sensed: attention goes to the closest, and a crowd never
/// churns memory.
///
/// Cheap pruning first (wave H2): a candidate outside every vision cone
/// (range + integer FOV), outside proximity and outside reach can never be
/// sensed, so it is dropped before the sort; line of sight (the expensive
/// part, the same for every vision sensor and for reach) is traced only for
/// the candidates actually visited, in the same nearest-first order, at most
/// once each. The detection check sees exactly the same calls in the same
/// order as a full scan. Without a [`ReachSense`] the scan is exactly the
/// vision/proximity scan it was before reach existed.
///
/// A robot (`robot`: its taken [`SensorReadings`] and the scratch) also
/// runs its LIDAR, and its camera and radar report the things in their
/// cones with line of sight (at most `limit` each, nearest first, even once
/// the memory's `limit` is reached), sensed automatically: no detection
/// check. Priority when several senses sense a thing: vision, camera,
/// radar, proximity, reach.
/// The LIDAR of a robot on its LIDAR tick ([`is_lidar_due`]): the scan into
/// `readings.lidar` (none without a LIDAR). Only the things the content's
/// [`LidarObstacles`] name stop a beam in their cell.
fn run_lidar(
    ctx: &Ctx,
    agent: &Agent,
    sensors: &Sensors,
    readings: &mut SensorReadings,
    work: &mut RobotWork,
) {
    let Some(lidar) = sensors.lidar_sensor() else {
        readings.lidar = None;
        return;
    };
    readings.lidar_tick = ctx.tick;
    let things = ctx
        .things
        .iter()
        .filter(|t| t.id != agent.id && t.kinds & ctx.obstacles != 0)
        .map(|t| t.pos);
    scan_lidar(
        LidarInput {
            grid: ctx.grid,
            things,
            entity: agent.entity,
            id: agent.id,
            pos: agent.pos,
            facing: agent.facing,
            lidar,
            seed: ctx.seed,
            tick: ctx.tick,
        },
        readings.lidar.get_or_insert_with(Default::default),
        &mut work.lidar,
    );
}

fn scan(
    ctx: &Ctx,
    agent: &Agent,
    sensors: &Sensors,
    reach: Option<ReachSense>,
    limit: usize,
    robot: Option<(&mut SensorReadings, &mut RobotWork)>,
) -> Vec<(SimId, Sensed)> {
    let Some((readings, work)) = robot else {
        return scan_targets(ctx, agent, sensors, reach, limit, None);
    };
    readings.tick = ctx.tick;
    work.camera_hits.clear();
    work.radar_hits.clear();
    let out = scan_targets(ctx, agent, sensors, reach, limit, Some(&mut *work));
    if sensors.camera_sensor().is_some() {
        camera::publish(readings, &work.camera_hits, &mut work.ids);
    } else {
        readings.camera.clear();
        readings.tracks.clear();
    }
    match sensors.radar_sensor() {
        Some(r) => radar::publish(readings, &r, &work.radar_hits, ctx.tick, &mut work.ids),
        None => {
            readings.radar.clear();
            readings.radar_prev.clear();
        }
    }
    out
}

/// One candidate of a scan.
struct Candidate {
    d2: i64,
    id: SimId,
    entity: Entity,
    pos: SimPos,
    kinds: u32,
    /// In a vision cone.
    cone: bool,
    /// In the camera's cone.
    camera: bool,
    /// In the radar's cone.
    radar: bool,
}

/// The targets part of [`scan`] (`hits`: a robot's camera and radar hits
/// are gathered there).
fn scan_targets(
    ctx: &Ctx,
    agent: &Agent,
    sensors: &Sensors,
    reach: Option<ReachSense>,
    limit: usize,
    mut hits: Option<&mut RobotWork>,
) -> Vec<(SimId, Sensed)> {
    let mut out: Vec<(SimId, Sensed)> = Vec::new();
    let reach_mm = reach.map(|r| r.radius_mm.max(0));
    let radius = sensors.scan_radius().max(reach_mm.unwrap_or(0));
    if limit == 0 || (radius <= 0 && sensors.proximity_radius().is_none() && reach_mm.is_none()) {
        return out;
    }
    // A robot's camera and radar (only when their hits are gathered).
    let camera = hits.as_ref().and(sensors.camera_sensor());
    let radar = hits.as_ref().and(sensors.radar_sensor());
    let camera2 = camera.map(|c| i64::from(c.range_mm.max(0)).pow(2));
    let radar2 = radar.map(|r| i64::from(r.range_mm.max(0)).pow(2));
    let proximity = sensors
        .proximity_radius()
        .map(|r| i64::from(r) * i64::from(r));
    let reach2 = reach_mm.map(|r| i64::from(r) * i64::from(r));
    // `(range², vision)`, only the sensors that can see anything.
    let visions: Vec<(i64, crate::sensors::Vision)> = sensors
        .visions()
        .map(|v| {
            let r = i64::from(v.range_mm.max(0));
            (r * r, v)
        })
        .collect();
    // Merge the sorted radius query with the sorted perceivables, keeping
    // only what a cone, proximity or reach could sense: `(d², id, entity,
    // pos, kinds, in a cone)`.
    let ids = ctx.grid.in_radius(agent.pos, radius);
    let mut candidates: Vec<Candidate> = Vec::new();
    let mut things = ctx.things.iter().peekable();
    for id in ids {
        while things.next_if(|t| t.id < id).is_some() {}
        let Some(t) = things.next_if(|t| t.id == id) else {
            continue;
        };
        if id == agent.id {
            continue;
        }
        let d2 = agent.pos.dist2(t.pos);
        let d = (
            i64::from(t.pos.x) - i64::from(agent.pos.x),
            i64::from(t.pos.z) - i64::from(agent.pos.z),
        );
        let cone = visions
            .iter()
            .any(|(r2, v)| d2 <= *r2 && v.sees_direction(agent.facing, d));
        let in_camera = camera
            .zip(camera2)
            .is_some_and(|(c, r2)| d2 <= r2 && c.sees_direction(agent.facing, d));
        let in_radar = radar
            .zip(radar2)
            .is_some_and(|(r, r2)| d2 <= r2 && r.sees_direction(agent.facing, d));
        if cone
            || in_camera
            || in_radar
            || proximity.is_some_and(|p2| d2 <= p2)
            || reach2.is_some_and(|r2| d2 <= r2)
        {
            candidates.push(Candidate {
                d2,
                id,
                entity: t.entity,
                pos: t.pos,
                kinds: t.kinds,
                cone,
                camera: in_camera,
                radar: in_radar,
            });
        }
    }
    candidates.sort_unstable_by_key(|c| (c.d2, c.id));
    let (mut camera_n, mut radar_n) = (0usize, 0usize);
    for c in candidates {
        let Candidate {
            d2,
            id,
            entity,
            pos,
            kinds,
            cone,
            ..
        } = c;
        let full = out.len() >= limit;
        let camera_open = camera.is_some() && camera_n < limit;
        let radar_open = radar.is_some() && radar_n < limit;
        if full && !camera_open && !radar_open {
            break;
        }
        let in_reach = reach2.is_some_and(|r2| d2 <= r2);
        // Traced at most once, and only when a sense needs it.
        let mut sight: Option<bool> = None;
        let mut los = || *sight.get_or_insert_with(|| line_of_sight(ctx.grid, agent.pos, pos));
        let seen_by_camera = c.camera && los();
        let seen_by_radar = c.radar && los();
        let dist = || i32::try_from(u64::try_from(d2).unwrap_or(0).isqrt()).unwrap_or(i32::MAX);
        let offset = (
            i64::from(pos.x) - i64::from(agent.pos.x),
            i64::from(pos.z) - i64::from(agent.pos.z),
        );
        let mut camera_confidence = 0;
        if let (Some(work), Some(cam)) = (hits.as_deref_mut(), camera)
            && seen_by_camera
        {
            camera_confidence = camera::confidence_ppm(i64::from(dist()), cam.range_mm);
            if camera_open {
                work.camera_hits.push(CameraHit {
                    id,
                    class: inspect::kind_name(ctx.world, entity),
                    bearing_mdeg: bearing_mdeg(agent.facing, offset),
                    distance_mm: dist(),
                    size_mm: camera::size_mm(ctx.world, entity),
                    confidence_ppm: camera_confidence,
                });
                camera_n += 1;
            }
        }
        if let Some(work) = hits.as_deref_mut()
            && seen_by_radar
            && radar_open
        {
            work.radar_hits.push(RadarHit {
                id,
                pos,
                range_mm: dist(),
                bearing_mdeg: bearing_mdeg(agent.facing, offset),
            });
            radar_n += 1;
        }
        if full {
            continue;
        }
        let via = if cone && los() {
            SenseKind::Vision
        } else if seen_by_camera {
            SenseKind::Camera
        } else if seen_by_radar {
            SenseKind::Radar
        } else if proximity.is_some_and(|p2| d2 <= p2) {
            SenseKind::Proximity
        } else if in_reach && los() {
            SenseKind::Reach
        } else {
            continue;
        };
        // Camera and radar are automatic: a sensor reports what is there.
        let confidence = match via {
            SenseKind::Camera => u16::try_from(camera_confidence / 1000).unwrap_or(CONFIDENCE_FULL),
            SenseKind::Radar => CONFIDENCE_FULL,
            _ => match ctx.detect(agent, Some((id, entity)), via, d2, None) {
                Some(c) => c,
                None => continue,
            },
        };
        out.push((
            id,
            Sensed {
                via,
                pos,
                kinds,
                confidence,
            },
        ));
    }
    out.sort_unstable_by_key(|(id, _)| *id);
    out
}

fn apply(
    memory: &mut PerceptionMemory,
    config: &PerceptionConfig,
    tick: u64,
    outcome: Outcome,
    log: &mut impl FnMut(JournalKind),
) {
    let perceived = |target: SimId, via: SenseKind| JournalKind::Perceived {
        target,
        via: via.name().to_owned(),
    };
    for (noise, percept) in outcome.heard {
        memory.last_noise = Some(HeardNoise {
            tick,
            pos: noise.pos,
            source: noise.source,
            kind: noise.kind,
        });
        let Some((id, s)) = percept else { continue };
        match memory.percepts.get_mut(&id) {
            // Seen or felt right now: that observation stays.
            Some(p) if p.sensed => {}
            Some(p) => {
                p.last_seen_tick = tick;
                p.pos = s.pos;
                p.via = s.via;
                p.confidence = s.confidence;
                p.kinds |= s.kinds;
            }
            None => {
                memory.percepts.insert(
                    id,
                    Percept {
                        last_seen_tick: tick,
                        pos: s.pos,
                        via: s.via,
                        confidence: s.confidence,
                        kinds: s.kinds,
                        sensed: false,
                    },
                );
                log(perceived(id, s.via));
            }
        }
    }

    if let Some(scan) = outcome.scan {
        for &(id, ref s) in &scan {
            let fresh = Percept {
                last_seen_tick: tick,
                pos: s.pos,
                via: s.via,
                confidence: s.confidence,
                kinds: s.kinds,
                sensed: true,
            };
            match memory.percepts.insert(id, fresh) {
                Some(old) if old.sensed => {}
                _ => log(perceived(id, s.via)),
            }
        }
        for (&id, p) in memory.percepts.iter_mut() {
            if p.sensed && scan.binary_search_by_key(&id, |(i, _)| *i).is_err() {
                p.sensed = false;
                log(JournalKind::Lost {
                    target: id,
                    forgotten: false,
                });
            }
        }
        let expired: Vec<SimId> = memory
            .percepts
            .iter()
            .filter(|(_, p)| tick.saturating_sub(p.last_seen_tick) > config.forget_after_ticks)
            .map(|(&id, _)| id)
            .collect();
        for id in expired {
            memory.percepts.remove(&id);
            log(JournalKind::Lost {
                target: id,
                forgotten: true,
            });
        }
    }

    let capacity = config.memory_capacity as usize;
    while memory.percepts.len() > capacity {
        let Some(oldest) = memory
            .percepts
            .iter()
            .min_by_key(|(id, p)| (p.last_seen_tick, **id))
            .map(|(&id, _)| id)
        else {
            break;
        };
        memory.percepts.remove(&oldest);
        log(JournalKind::Lost {
            target: oldest,
            forgotten: true,
        });
    }
}

fn write_facts(world: &mut World, agent: &Agent, facts: PerceptionFacts) {
    let nearest = world
        .get::<PerceptionMemory>(agent.entity)
        .and_then(|m| m.nearest(agent.pos, facts.kinds, true));
    let (target, pos) = match nearest {
        Some((id, p)) => (BbValue::U32(id.0), BbValue::Pos(p.pos.x, p.pos.z)),
        None => (BbValue::None, BbValue::None),
    };
    let Some(mut bb) = world.get_mut::<Blackboard>(agent.entity) else {
        return;
    };
    for (slot, value) in [(facts.target_slot, target), (facts.pos_slot, pos)] {
        if (slot as usize) < bb.slots.len() && bb.get(BbKey(slot)) != value {
            bb.set(BbKey(slot), value);
        }
    }
}

/// `NpcSet::Cleanup`: an agent that moved this tick (`SimPos` ≠ the tick's
/// starting `PrevSimPos`) faces its movement direction, unless it has
/// [`FixedFacing`].
pub fn facing_from_movement_system(
    mut agents: Query<(&SimPos, &PrevSimPos, &mut Facing), Without<FixedFacing>>,
) {
    for (pos, prev, mut facing) in &mut agents {
        if *pos != prev.0 {
            let next = Facing::towards(prev.0, *pos);
            if *facing != next {
                *facing = next;
            }
        }
    }
}
