//! `npc_perception`: opt-in sensors and perception memory
//! (actor-capabilities spec §4.2, plan wave D1).
//!
//! Add [`PerceptionPlugin`] next to `NpcPlugins` (it needs the kernel's
//! `SpatialPlugin`, which `NpcPlugins` includes), put [`Sensors`] on agents
//! and [`Perceivable`] on things that can be seen. Without the plugin
//! nothing here runs, is registered or hashed: the app behaves exactly as
//! before.
//!
//! ## Senses
//!
//! - **Vision** ([`Vision`]): range in mm and a half angle in whole degrees
//!   around the agent's [`Facing`]. The angle test ([`in_fov`]) uses an
//!   integer dot product and squared comparisons against `cos²` of the half
//!   angle (an offline table, [`COS2_PPM`]): no trigonometry at runtime,
//!   inclusive edges. Line of sight ([`line_of_sight`]) traces the grid
//!   segment (`SpatialGrid::trace_segment`, a conservative supercover) and is
//!   blocked by any occluding cell strictly between the two cells
//!   (`GridOccluder` entities or static cells). Occluder changes need no
//!   special handling: line of sight is traced afresh on every scan.
//! - **Proximity** ([`Proximity`]): a radius, any direction, no line of sight.
//! - **Reach** ([`ReachSense`], opt-in, a component next to [`Sensors`]): a
//!   radius, any direction, **with** line of sight: what the agent could
//!   touch, never through a wall. Snapshot-only ([`REACH_SENSE`]): it joins
//!   neither the state nor the content hash, so worlds without it are
//!   unchanged.
//! - **Hearing** ([`Hearing`]): a radius; reacts to [`Noise`] stimuli, which
//!   any crate emits with [`emit_noise`] / [`EmitNoise`] (no-ops without the
//!   plugin).
//!
//! - **Robot sensors** (plan Task 12, spec G10; [`Lidar`], [`Camera`],
//!   [`Radar`] in [`Sensors`]): they behave like their real counterparts and
//!   report ROS-shaped data in the agent's [`SensorReadings`]
//!   (snapshot-only): the LIDAR a [`LaserScan`] (ranges only, no
//!   identities, no memory; [`lidar`]), the camera [`CameraDetection`]s
//!   (detections only, never images; [`camera`]) and the radar
//!   [`RadarTrack`]s ([`radar`]). Camera and radar feed the memory
//!   (`SenseKind::Camera`, `SenseKind::Radar`) without a detection check.
//!
//! ## A tick (`NpcSet::Sense`, [`NpcPerceptionSet`])
//!
//! [`perception_system`] (exclusive) hears every pending noise for every
//! listener (every tick), and scans vision/proximity/reach **staggered**: agent
//! `id` scans when `(id + tick) % stagger == 0` ([`PerceptionConfig::stagger`],
//! default 4). Every candidate passes the active detection check
//! ([`detect`], default `"automatic"`; a rules plugin can plug in a
//! Perception roll by name). Results land in the agent's bounded
//! [`PerceptionMemory`] (`BTreeMap<SimId, Percept>`), forgotten
//! `forget_after_ticks` after they were last sensed, oldest evicted beyond
//! `memory_capacity` (a scan senses at most that many, nearest first). Journal: `Perceived { target, via }` when a target
//! becomes sensed, `Lost { target, forgotten }` when it stops being sensed
//! (`false`) or is forgotten (`true`). [`PerceptionFacts`] mirrors the
//! nearest sensed percept into the blackboard. At the end of the tick
//! ([`facing_from_movement_system`], `NpcSet::Cleanup`) agents that moved
//! face their movement unless they have [`FixedFacing`].
//!
//! ## Fog of war ([`fog`])
//!
//! Actors with [`RevealsFog`] keep the cells they have explored
//! ([`ExploredCells`]), the cells they sense now ([`VisibleCells`]) and the
//! last-seen state of the statics there ([`SeenStatics`]; what counts as
//! static is content's [`FogStatics`]). Updated by [`fog_system`] right
//! after the perception systems ([`NpcFogSet`]), on the same stagger ticks
//! (perf mechanic `fog`); snapshot-only, so it changes no hash.
//!
//! ## Behaviour trees
//!
//! Conditions `sees_kind`, `remembers_kind` and `heard_noise` ([`conds`]),
//! plus the generic `sees::<MASK>`, `remembers::<MASK>`, `heard::<MASK>` for
//! content to register under its own names.
//!
//! ## Determinism and hashing
//!
//! Integer-only; agents in `SimId` order; spatial queries return sorted
//! `SimId`s; noises are sorted before use; memory is a `BTreeMap`. The
//! plugin registers its state for the state hash ([`register_perception_hash`]:
//! `sensors`, `facing`, `fixed_facing`, `perceivable`, `perception_memory`,
//! `perception_facts`, resources `perception_config` and `pending_noises`
//! (noises written after the pass, heard next tick), plus the spatial grid's
//! static layer and occluders, which line of sight reads) and the detection
//! checks in the content hash. That changes the content hash of any app that
//! adds it, which is why it is opt-in.

#![deny(
    clippy::float_arithmetic,
    clippy::float_cmp,
    clippy::iter_over_hash_type,
    clippy::disallowed_types,
    clippy::disallowed_methods
)]

pub mod camera;
pub mod conds;
pub mod detect;
pub mod fog;
pub mod lidar;
pub mod memory;
pub mod noise;
pub mod radar;
pub mod readings;
pub mod sensors;
pub mod system;

use bevy_app::prelude::*;
use bevy_ecs::message::Messages;
use bevy_ecs::prelude::*;
use npc_kernel::hash::{Fnv64, SimHashRegistry, SimHashResource};
use npc_kernel::registry::{KernelAppExt, is_frozen};
use npc_kernel::schedule::NpcSimTick;
use npc_kernel::snapshot::SnapshotAppExt;
use npc_kernel::spatial::{SpatialGrid, register_spatial_hash};
use npc_kernel::{NpcSet, SimIndex};
use serde_json::{Value, json};

pub use camera::Detection as CameraDetection;
pub use detect::{AUTOMATIC, Detection, DetectionCheckFn, DetectionChecks, NoiseIndex};
pub use fog::{
    EXPLORED_CELLS, ExploredCells, FogResets, FogScratch, FogStatics, KindBits, REVEALS_FOG,
    RevealsFog, SEEN_STATICS, SeenState, SeenStatics, StaticStateFn, VISIBLE_CELLS, VisibleCells,
    fog_system, reveal_cells, reveal_reach_cells,
};
pub use lidar::{LaserScan, LidarObstacles};
pub use memory::{
    CONFIDENCE_FULL, HeardNoise, Percept, PerceptionFacts, PerceptionMemory, SenseKind,
};
pub use noise::{EmitNoise, Noise, emit_noise};
pub use radar::{RadarEcho, RadarTrack};
pub use readings::{
    SENSOR_READINGS, SensorReadings, atan2_mdeg, bearing_mdeg, dir_ppm, facing_angle_mdeg,
};
pub use sensors::{
    ALL_KINDS, COS2_PPM, COS2_SCALE, Camera, Facing, FixedFacing, Hearing, Lidar, Perceivable,
    Proximity, REACH_SENSE, Radar, ReachSense, Sensor, Sensors, Vision, cos2_ppm_of, in_fov,
};
pub use system::{
    RobotScratch, facing_from_movement_system, is_due, is_lidar_due, line_of_sight,
    perception_system,
};

/// Default [`PerceptionConfig::stagger`].
pub const DEFAULT_STAGGER: u32 = 4;
/// Default [`PerceptionConfig::forget_after_ticks`] (10 s at 20 Hz).
pub const DEFAULT_FORGET_AFTER: u64 = 200;
/// Default [`PerceptionConfig::memory_capacity`].
pub const DEFAULT_MEMORY_CAPACITY: u32 = 32;
/// Default [`PerceptionConfig::noise_window_ticks`] (1 s at 20 Hz).
pub const DEFAULT_NOISE_WINDOW: u64 = 20;

/// Tuning. Insert before adding the plugin to change it (the plugin uses
/// `init_resource`). Hashed as `perception_config`.
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub struct PerceptionConfig {
    /// Vision/proximity scans every `stagger` ticks per agent (1 = every tick).
    pub stagger: u32,
    /// A percept is forgotten once it was last sensed more than this many
    /// ticks ago (checked on the agent's scan ticks).
    pub forget_after_ticks: u64,
    /// At most this many percepts per agent (the oldest are evicted); a
    /// scan also senses at most this many, nearest first (attention).
    pub memory_capacity: u32,
    /// `heard_noise` looks back this many ticks.
    pub noise_window_ticks: u64,
}

impl Default for PerceptionConfig {
    fn default() -> Self {
        Self {
            stagger: DEFAULT_STAGGER,
            forget_after_ticks: DEFAULT_FORGET_AFTER,
            memory_capacity: DEFAULT_MEMORY_CAPACITY,
            noise_window_ticks: DEFAULT_NOISE_WINDOW,
        }
    }
}

impl SimHashResource for PerceptionConfig {
    const NAME: &'static str = "perception_config";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.u32(self.stagger);
        h.u64(self.forget_after_ticks);
        h.u32(self.memory_capacity);
        h.u64(self.noise_window_ticks);
    }
}

/// The perception systems' set inside `NpcSet::Sense` (order other `Sense`
/// systems against it).
#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct NpcPerceptionSet;

/// The fog-of-war system's set inside `NpcSet::Sense`, after
/// [`NpcPerceptionSet`] ([`fog_system`]).
#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct NpcFogSet;

/// Registration for perception hooks.
pub trait PerceptionAppExt {
    /// Registers a detection check by name (replacing one of that name).
    fn register_detection_check(&mut self, name: &'static str, f: DetectionCheckFn) -> &mut Self;
    /// Makes the check `name` the active one (checked at freeze).
    fn use_detection_check(&mut self, name: &'static str) -> &mut Self;
    /// Says which things fog of war records as statics ([`SeenStatics`]),
    /// replacing the previous [`FogStatics`]. Not hashed: it only shapes the
    /// snapshot-only fog record of [`RevealsFog`] actors.
    fn set_fog_statics(&mut self, statics: FogStatics) -> &mut Self;
    /// Says which perceivable things stop a LIDAR beam ([`LidarObstacles`],
    /// replacing the previous one). Not hashed: it only shapes the
    /// snapshot-only [`SensorReadings`].
    fn set_lidar_obstacles(&mut self, obstacles: LidarObstacles) -> &mut Self;
}

impl PerceptionAppExt for App {
    fn register_detection_check(&mut self, name: &'static str, f: DetectionCheckFn) -> &mut Self {
        assert!(
            !is_frozen(self.world()),
            "cannot register detection check `{name}`: content is already frozen"
        );
        self.world_mut()
            .get_resource_or_init::<DetectionChecks>()
            .checks
            .insert(name, f);
        self
    }

    fn use_detection_check(&mut self, name: &'static str) -> &mut Self {
        assert!(
            !is_frozen(self.world()),
            "cannot select detection check `{name}`: content is already frozen"
        );
        self.world_mut()
            .get_resource_or_init::<DetectionChecks>()
            .active = name;
        self
    }

    fn set_fog_statics(&mut self, statics: FogStatics) -> &mut Self {
        self.insert_resource(statics);
        self
    }

    fn set_lidar_obstacles(&mut self, obstacles: LidarObstacles) -> &mut Self {
        self.insert_resource(obstacles);
        self
    }
}

/// Adds the perception state to the state hash (idempotent), plus the
/// spatial grid's static layer and occluders. Changes the content hash; the
/// plugin calls it.
pub fn register_perception_hash(app: &mut App) {
    register_spatial_hash(app);
    let registry = app.world_mut().get_resource_or_init::<SimHashRegistry>();
    if registry.components.iter().any(|c| c.name == "sensors") {
        return;
    }
    app.register_sim_component::<Sensors>()
        .register_sim_component::<Facing>()
        .register_sim_component::<FixedFacing>()
        .register_sim_component::<Perceivable>()
        .register_sim_component::<PerceptionFacts>()
        .register_sim_component_with::<PerceptionMemory>(|world, entity| {
            world.get::<PerceptionMemory>(entity).map(|m| m.to_json())
        })
        .register_sim_resource::<PerceptionConfig>();
    app.world_mut()
        .get_resource_or_init::<SimHashRegistry>()
        .add_resource_fn(PENDING_NOISES, hash_pending_noises);
}

/// State-hash name of the noises waiting in `Messages<Noise>` (written after
/// `NpcSet::Sense`, heard next tick).
pub const PENDING_NOISES: &str = "pending_noises";

/// Hashes the pending noises, sorted by [`Noise::sort_key`] (absent without
/// the buffer). Usually empty at a tick boundary unless something made a
/// noise after the perception pass (e.g. an interaction completing).
fn hash_pending_noises(world: &World, h: &mut Fnv64) -> bool {
    let Some(messages) = world.get_resource::<Messages<Noise>>() else {
        return false;
    };
    let mut cursor = messages.get_cursor();
    let mut noises: Vec<Noise> = cursor.read(messages).copied().collect();
    noises.sort_by_key(Noise::sort_key);
    h.u32(u32::try_from(noises.len()).expect("too many noises"));
    for n in &noises {
        h.opt(n.source.as_ref(), |h, s| h.u32(s.0));
        h.i32(n.pos.x);
        h.i32(n.pos.z);
        h.i32(n.radius_mm);
        h.u32(n.kind);
    }
    true
}

/// Snapshot save of the pending noises (as written; the hearing pass sorts
/// them), under [`PENDING_NOISES`].
fn save_pending_noises(world: &World) -> Option<Value> {
    let messages = world.get_resource::<Messages<Noise>>()?;
    let mut cursor = messages.get_cursor();
    let noises: Vec<Noise> = cursor.read(messages).copied().collect();
    Some(serde_json::to_value(noises).expect("noises serialize to JSON"))
}

/// Replaces the noise buffer with the saved noises.
fn load_pending_noises(world: &mut World, value: Value) -> Result<(), String> {
    let noises: Vec<Noise> = serde_json::from_value(value).map_err(|e| e.to_string())?;
    let mut messages = world.get_resource_or_init::<Messages<Noise>>();
    messages.clear();
    for n in noises {
        messages.write(n);
    }
    Ok(())
}

/// Contributes the detection checks to the content hash and validates the
/// active one.
fn freeze_detection(world: &mut World) -> Vec<(String, String)> {
    let checks = world.get_resource_or_init::<DetectionChecks>();
    assert!(
        checks.checks.contains_key(checks.active),
        "active detection check `{}` is not registered (known: {:?})",
        checks.active,
        checks.checks.keys().collect::<Vec<_>>()
    );
    let mut out: Vec<(String, String)> = checks
        .checks
        .keys()
        .map(|k| ("detection_check".to_owned(), (*k).to_owned()))
        .collect();
    out.push((
        "detection_check_active".to_owned(),
        checks.active.to_owned(),
    ));
    out
}

/// The perception capability (see the crate docs). Installs
/// [`PerceptionConfig`], [`DetectionChecks`], the `Messages<Noise>` buffer
/// (drained by the perception system, not by `First`), the hash and
/// content-hash registrations, the conditions `sees_kind`, `remembers_kind`
/// and `heard_noise`, [`perception_system`] in `NpcSet::Sense` and
/// [`facing_from_movement_system`] in `NpcSet::Cleanup`.
pub struct PerceptionPlugin;

impl Plugin for PerceptionPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SpatialGrid>()
            .init_resource::<PerceptionConfig>()
            .init_resource::<DetectionChecks>()
            .init_resource::<Messages<Noise>>();
        register_perception_hash(app);
        // Snapshot-only (ADR 0013): no presence byte in the state hash, no
        // content-hash entry, so worlds without reach keep their hashes.
        app.register_snapshot_component_named::<ReachSense>(REACH_SENSE);
        // The robot sensors' readings (plan Task 12): snapshot-only too.
        app.register_snapshot_component_named::<SensorReadings>(SENSOR_READINGS);
        // Fog of war (plan Task 9): snapshot-only too, so the explored
        // ground survives a resume and no hash moves.
        app.init_resource::<LidarObstacles>();
        app.init_resource::<FogStatics>()
            .register_snapshot_component_named::<RevealsFog>(REVEALS_FOG)
            .register_snapshot_component_named::<ExploredCells>(EXPLORED_CELLS)
            .register_snapshot_component_named::<VisibleCells>(VISIBLE_CELLS)
            .register_snapshot_component_named::<SeenStatics>(SEEN_STATICS)
            .add_snapshot_rebuild("npc_perception.fog", fog::fog_after_load);
        app.register_snapshot_resource_with(
            PENDING_NOISES,
            save_pending_noises,
            load_pending_noises,
        );
        app.register_cond("sees_kind", conds::sees_kind)
            .register_cond("remembers_kind", conds::remembers_kind)
            .register_cond("heard_noise", conds::heard_noise)
            .add_freeze_hook("npc_perception", freeze_detection)
            .configure_sets(
                NpcSimTick,
                (
                    NpcPerceptionSet.in_set(NpcSet::Sense),
                    NpcFogSet.in_set(NpcSet::Sense).after(NpcPerceptionSet),
                ),
            )
            .add_systems(
                NpcSimTick,
                (
                    perception_system.in_set(NpcPerceptionSet),
                    fog_system.in_set(NpcFogSet),
                    facing_from_movement_system.in_set(NpcSet::Cleanup),
                ),
            );
    }
}

/// Inspect section for one unit: its sensors, facing, interest and memory
/// (`None` without `Sensors`). Content inspectors embed it; the generic
/// inspector already shows the registered components.
pub fn perception_json(world: &World, entity: Entity) -> Option<Value> {
    let sensors = world.get::<Sensors>(entity)?;
    let mut out = json!({
        "sensors": sensors.sensors,
        "interest": sensors.interest,
        "reach": world.get::<ReachSense>(entity),
        "facing": world.get::<Facing>(entity),
        "memory": world.get::<PerceptionMemory>(entity).map(PerceptionMemory::to_json),
    });
    // Only robots have readings (the key is absent otherwise, so older
    // inspect output is unchanged).
    if let (Some(r), Some(map)) = (world.get::<SensorReadings>(entity), out.as_object_mut()) {
        map.insert(
            "readings".to_owned(),
            serde_json::to_value(r).unwrap_or(Value::Null),
        );
    }
    Some(out)
}

/// [`perception_json`] by `SimId`.
pub fn perception_json_of(world: &World, id: npc_kernel::SimId) -> Option<Value> {
    let entity = world.get_resource::<SimIndex>()?.get(id)?;
    perception_json(world, entity)
}
