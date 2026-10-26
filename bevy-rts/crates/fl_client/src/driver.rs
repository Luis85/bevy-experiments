//! [`LocalDriver`]: the single-player [`TickDriver`] (spec §5.3, ADR-0002).
use std::collections::BTreeMap;
use std::time::Duration;

use bevy::prelude::*;
use fl_protocol::{PlayerCommand, PlayerId, SIM_TICK_HZ, Tick};
use fl_view::{CommandDraft, CommandStamper, TickDriver, TickInput};

/// The length of one sim tick of wall-clock time at speed ×1.
pub const TICK_LENGTH: Duration = Duration::from_nanos(1_000_000_000 / SIM_TICK_HZ as u64);
/// The slowest game speed.
pub const MIN_SPEED: f32 = 0.25;
/// The fastest game speed.
pub const MAX_SPEED: f32 = 8.0;
/// The most wall-clock time one [`LocalDriver::advance`] counts, so a stall (a breakpoint, a
/// window drag) can't queue a burst of catch-up ticks.
pub const MAX_ADVANCE: Duration = Duration::from_millis(250);
/// Ticks between issuing a command and executing it.
const INPUT_DELAY: u32 = 1;

/// Ticks the sim from a wall-clock accumulator, with pause, single step and speed ×0.25–×8.
///
/// Commands are stamped with `exec_tick = next_tick + 1` and the player's next seq, held, and
/// delivered in the [`TickInput`] of exactly their `exec_tick`, paused or not.
#[derive(Resource, Debug, Clone)]
pub struct LocalDriver {
    stamper: CommandStamper,
    /// The tick the next poll delivers input for.
    next: Tick,
    /// Scaled wall-clock time not yet turned into ticks.
    accumulator: Duration,
    paused: bool,
    /// Ticks still allowed while paused.
    steps: u32,
    speed: f32,
}

impl LocalDriver {
    /// A running driver at speed ×1 whose first tick is `next`.
    #[must_use]
    pub fn new(next: Tick) -> Self {
        LocalDriver {
            stamper: CommandStamper::default(),
            next,
            accumulator: Duration::ZERO,
            paused: false,
            steps: 0,
            speed: 1.0,
        }
    }

    /// Adds `dt × speed` to the accumulator, unless paused; `dt` counts at most [`MAX_ADVANCE`].
    pub fn advance(&mut self, dt: Duration) {
        if !self.paused {
            self.accumulator += dt.min(MAX_ADVANCE).mul_f32(self.speed);
        }
    }

    /// The wall-clock time one tick takes at the current speed.
    #[must_use]
    pub fn tick_period(&self) -> Duration {
        TICK_LENGTH.div_f32(self.speed)
    }

    /// Stops ticking; the accumulator and the interpolation fraction freeze.
    pub fn pause(&mut self) {
        self.paused = true;
    }

    /// Resumes ticking and drops any steps not yet taken.
    pub fn resume(&mut self) {
        self.paused = false;
        self.steps = 0;
    }

    #[must_use]
    pub fn is_paused(&self) -> bool {
        self.paused
    }

    /// While paused, allows `n` more ticks. Ignored while running.
    pub fn step(&mut self, n: u32) {
        if self.paused {
            self.steps = self.steps.saturating_add(n);
        }
    }

    /// Sets the game speed, clamped to `[0.25, 8.0]`.
    pub fn set_speed(&mut self, x: f32) {
        self.speed = if x.is_nan() {
            1.0
        } else {
            x.clamp(MIN_SPEED, MAX_SPEED)
        };
    }

    #[must_use]
    pub fn speed(&self) -> f32 {
        self.speed
    }

    /// How far the view is into the next tick: accumulator / tick length, in `[0, 1)`.
    #[must_use]
    pub fn overstep_fraction(&self) -> f32 {
        let fraction = self.accumulator.as_secs_f32() / TICK_LENGTH.as_secs_f32();
        fraction.clamp(0.0, 1.0 - f32::EPSILON)
    }

    /// Resets the driver after a load: the next tick is `next`, it holds exactly `pending`, and
    /// each player's seq continues after `last_seq` (and after its pending commands).
    pub fn restore(
        &mut self,
        next: Tick,
        pending: Vec<PlayerCommand>,
        last_seq: &BTreeMap<PlayerId, u32>,
    ) {
        self.next = next;
        self.stamper.restore(pending, last_seq);
        self.accumulator = Duration::ZERO;
        self.steps = 0;
    }

    /// The input of the next tick, and advances `next`.
    fn emit(&mut self) -> TickInput {
        let tick = self.next;
        self.next = tick.next();
        TickInput {
            tick,
            commands: self.stamper.take_due(tick),
        }
    }
}

impl TickDriver for LocalDriver {
    fn next_tick(&self) -> Tick {
        self.next
    }

    fn input_delay(&self) -> u32 {
        INPUT_DELAY
    }

    fn issue(&mut self, draft: CommandDraft) -> PlayerCommand {
        let exec_tick = Tick(self.next.0 + INPUT_DELAY);
        self.stamper.stamp(exec_tick, draft)
    }

    fn poll(&mut self) -> Option<TickInput> {
        if self.paused {
            if self.steps == 0 {
                return None;
            }
            self.steps -= 1;
        } else {
            if self.accumulator < TICK_LENGTH {
                return None;
            }
            self.accumulator -= TICK_LENGTH;
        }
        Some(self.emit())
    }

    fn pending(&self) -> Vec<PlayerCommand> {
        self.stamper.held()
    }
}
