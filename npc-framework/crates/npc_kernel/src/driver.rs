//! The host-safe driver: runs [`NpcSimTick`] from a schedule the **host**
//! chooses, at the tick rate the host chooses, without touching any host
//! resource (ADR 0008).
//!
//! - [`NpcDriverPlugin`] adds one exclusive system ([`NpcDriverSet`]) to the
//!   chosen schedule (`FixedUpdate`, `Update`, ...). It reads that schedule's
//!   `Time` delta, accumulates it in its own [`SimTime`] and runs whole ticks
//!   of `1 / SimRate.hz`. It **never writes `Time<Fixed>`** or any other host
//!   resource, so a host with physics at 64 Hz keeps its 64 Hz.
//! - [`SimControl`] (pause, speed multiplier, single steps) is a plain
//!   resource: keyboard or UI bindings belong to the app.
//! - [`DeterminismMode`]: `Strict` (default) keeps ambiguity detection =
//!   `Error` on [`NpcSimTick`] and integer positions owned by the sim; the
//!   replay/hash checks are meaningful. `Relaxed` lets a host own positions:
//!   it writes [`HostPosition`] (e.g. from physics) and the sim copies it into
//!   `SimPos` at the start of each tick (`NpcSet::Inputs`). Replays are then
//!   only as deterministic as the host's mover, and ambiguities only warn.
//!
//! Sim systems never see the host clock: the driver turns host time into a
//! count of ticks, and a tick only ever reads `SimClock`.

use std::time::Duration;

use bevy_app::prelude::*;
use bevy_ecs::intern::Interned;
use bevy_ecs::prelude::*;
use bevy_ecs::schedule::ScheduleLabel;

use crate::clock::{DEFAULT_TICK_HZ, SimClock};
use crate::math::SimPos;
use crate::registry::ensure_frozen;
use crate::schedule::NpcSimTick;

/// Ticks per simulated second.
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub struct SimRate {
    pub hz: u32,
}

impl Default for SimRate {
    fn default() -> Self {
        Self {
            hz: DEFAULT_TICK_HZ,
        }
    }
}

impl SimRate {
    /// Length of one tick in host time.
    pub fn period(&self) -> Duration {
        Duration::from_nanos(1_000_000_000 / u64::from(self.hz.max(1)))
    }
}

/// The driver's own accumulator (never the host's `Time<Fixed>`).
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SimTime {
    /// Host time not yet turned into ticks (always `< period` after a run,
    /// unless the per-run cap was hit).
    pub accumulated: Duration,
    /// Ticks the last driver run executed.
    pub ticks_last_run: u32,
    /// Ticks the driver ran since startup.
    pub ticks_driven: u64,
}

impl SimTime {
    /// How far the host clock is into the next tick, in `0..=1_000_000`
    /// (millionths): the interpolation factor for presentation
    /// (`PrevSimPos → SimPos`). Integer, so the kernel stays float-free; the
    /// view converts it.
    pub fn overstep_ppm(&self, rate: &SimRate) -> u32 {
        let period = rate.period().as_nanos().max(1);
        let ppm = self.accumulated.as_nanos().saturating_mul(1_000_000) / period;
        u32::try_from(ppm.min(1_000_000)).unwrap_or(1_000_000)
    }
}

/// Pause, speed and single steps. Bind keys or UI to it in the app.
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub struct SimControl {
    pub paused: bool,
    /// Sim seconds per host second (0 stops like a pause).
    pub multiplier: u32,
    /// Ticks to run on the next driver run even while stopped (single step).
    pub pending_steps: u32,
}

impl Default for SimControl {
    fn default() -> Self {
        Self {
            paused: false,
            multiplier: 1,
            pending_steps: 0,
        }
    }
}

impl SimControl {
    /// True when the sim does not advance on its own (paused or 0×).
    pub fn is_stopped(&self) -> bool {
        self.paused || self.multiplier == 0
    }

    /// Requests one tick on the next driver run (meant for a stopped sim).
    pub fn step(&mut self) {
        self.pending_steps = self.pending_steps.saturating_add(1);
    }
}

/// Strict (default) or Relaxed determinism; see the module docs.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DeterminismMode {
    #[default]
    Strict,
    Relaxed,
}

/// Relaxed mode only: the host's position for this sim entity (e.g. read
/// back from physics), copied into `SimPos` at the start of every tick.
/// Ignored in Strict mode.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct HostPosition(pub SimPos);

fn relaxed(mode: Option<Res<DeterminismMode>>) -> bool {
    mode.is_some_and(|m| *m == DeterminismMode::Relaxed)
}

/// `NpcSet::Inputs`: `SimPos = HostPosition` (Relaxed mode only).
pub fn apply_host_positions_system(
    mode: Option<Res<DeterminismMode>>,
    mut q: Query<(&HostPosition, &mut SimPos)>,
) {
    if !relaxed(mode) {
        return;
    }
    for (host, mut pos) in &mut q {
        if *pos != host.0 {
            *pos = host.0;
        }
    }
}

/// Called after every tick the driver runs (outside `NpcSimTick`), e.g. to
/// close a profiler frame per tick. Tooling only: must not change sim state.
pub type TickObserver = fn(&mut World);

/// Observers run after each driven tick, sorted by name.
#[derive(Resource, Clone, Default)]
pub struct TickObservers {
    pub observers: Vec<(&'static str, TickObserver)>,
}

impl TickObservers {
    pub fn add(&mut self, name: &'static str, f: TickObserver) {
        match self.observers.binary_search_by_key(&name, |(n, _)| n) {
            Ok(i) => self.observers[i].1 = f,
            Err(i) => self.observers.insert(i, (name, f)),
        }
    }
}

/// The driver system's set, for hosts that want to order against it.
#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct NpcDriverSet;

/// How the driver turns schedule runs into ticks.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Pacing {
    /// Accumulate the schedule's `Time` delta × `SimControl.multiplier` and
    /// run whole ticks of `1 / SimRate.hz` (real-time play).
    #[default]
    HostTime,
    /// Run exactly `SimControl.multiplier` ticks per schedule run, ignoring
    /// time (headless servers and tests: deterministic frame stepping).
    EveryRun,
}

/// Drives [`NpcSimTick`] from `schedule` at `tick_hz` (see module docs).
/// Inserts `SimRate`, `SimTime`, `SimControl` and `TickObservers` with
/// `init_resource` (values the app inserted first are kept).
pub struct NpcDriverPlugin {
    pub tick_hz: u32,
    pub schedule: Interned<dyn ScheduleLabel>,
    /// Most ticks one driver run may execute; extra accumulated time is
    /// dropped (no spiral of death after a long frame).
    pub max_ticks_per_run: u32,
    pub pacing: Pacing,
}

impl NpcDriverPlugin {
    pub fn new(tick_hz: u32, schedule: impl ScheduleLabel) -> Self {
        Self {
            tick_hz,
            schedule: schedule.intern(),
            max_ticks_per_run: 100,
            pacing: Pacing::HostTime,
        }
    }
}

/// Settings of the driver system.
#[derive(Resource, Clone, Copy, Debug)]
pub struct DriverSettings {
    pub max_ticks_per_run: u32,
    pub pacing: Pacing,
}

impl Plugin for NpcDriverPlugin {
    fn build(&self, app: &mut App) {
        let hz = self.tick_hz;
        assert!(hz > 0, "NpcDriverPlugin: tick_hz must be > 0");
        if !app.world().contains_resource::<SimRate>() {
            app.insert_resource(SimRate { hz });
        }
        app.init_resource::<SimTime>()
            .init_resource::<SimControl>()
            .init_resource::<TickObservers>()
            .insert_resource(DriverSettings {
                max_ticks_per_run: self.max_ticks_per_run.max(1),
                pacing: self.pacing,
            })
            .add_systems(self.schedule, drive_sim.in_set(NpcDriverSet));
    }
}

/// The schedule's `Time` delta (the virtual clock in `Update`, the fixed
/// step in `FixedUpdate`), or zero without a time plugin.
#[allow(clippy::disallowed_types)]
// determinism: the driver is the one boundary that reads host time; it only
// decides *how many* ticks run, never what a tick does.
fn host_delta(world: &World) -> Duration {
    world
        .get_resource::<bevy_time::Time>()
        .map_or(Duration::ZERO, bevy_time::Time::delta)
}

/// Runs one tick plus the tick observers. False when the sim schedule is
/// missing.
fn run_one(world: &mut World, observers: &[(&'static str, TickObserver)]) -> bool {
    if world.try_run_schedule(NpcSimTick).is_err() {
        return false;
    }
    for (_, f) in observers {
        f(world);
    }
    true
}

/// The exclusive driver system. Never panics when the sim is not installed:
/// no `SimClock` or no `NpcSimTick` schedule means "skip".
pub fn drive_sim(world: &mut World) {
    if !world.contains_resource::<SimClock>() {
        return;
    }
    ensure_frozen(world);
    let rate = world.get_resource::<SimRate>().copied().unwrap_or_default();
    let settings = world
        .get_resource::<DriverSettings>()
        .copied()
        .unwrap_or(DriverSettings {
            max_ticks_per_run: 100,
            pacing: Pacing::HostTime,
        });
    let limit = settings.max_ticks_per_run;
    let delta = host_delta(world);
    let observers = world
        .get_resource::<TickObservers>()
        .map(|o| o.observers.clone())
        .unwrap_or_default();
    let mut control = world
        .get_resource::<SimControl>()
        .copied()
        .unwrap_or_default();
    let mut time = world.get_resource::<SimTime>().copied().unwrap_or_default();

    let mut ticks: u32 = 0;
    if control.is_stopped() {
        time.accumulated = Duration::ZERO;
        let steps = std::mem::take(&mut control.pending_steps).min(limit);
        if let Some(mut c) = world.get_resource_mut::<SimControl>() {
            c.pending_steps = 0;
        }
        for _ in 0..steps {
            if !run_one(world, &observers) {
                break;
            }
            ticks += 1;
        }
    } else if settings.pacing == Pacing::EveryRun {
        for _ in 0..control.multiplier.min(limit) {
            if !run_one(world, &observers) {
                break;
            }
            ticks += 1;
        }
    } else {
        let period = rate.period();
        time.accumulated += delta.saturating_mul(control.multiplier);
        while time.accumulated >= period && ticks < limit {
            time.accumulated -= period;
            if !run_one(world, &observers) {
                break;
            }
            ticks += 1;
        }
        if ticks >= limit && time.accumulated >= period {
            // Too far behind: drop the backlog instead of spiralling.
            time.accumulated = Duration::ZERO;
        }
    }
    time.ticks_last_run = ticks;
    time.ticks_driven += u64::from(ticks);
    world.insert_resource(time);
}
