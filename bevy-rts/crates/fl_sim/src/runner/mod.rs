//! [`SimRunner`]: owns the sim `World` and steps it, inline or on its own thread, and the
//! [`TickDriver`]s that feed it (spec §5.3, ADR-0002, ADR-0007).
//!
//! - Inline: [`SimRunner::step`] and [`SimRunner::run`] on the caller's thread (tests,
//!   `fl_cli`, a future server).
//! - Threaded: [`SimRunner::spawn_threaded`] moves the runner onto the `fl-sim` thread behind
//!   a [`RunnerPort`](fl_view::RunnerPort) (the client).
//!
//! Drivers: [`TestDriver`] (explicit, input delay 0), [`ReplayDriver`] (a recorded input
//! stream) and [`LockstepDriver`] (waits for every peer's batch, over a [`NetSession`]; M0
//! ships the in-process [`loopback`]).

mod lockstep;
mod replay_driver;
mod test_driver;
mod threaded;

use std::collections::VecDeque;

use bevy_app::App;
use bevy_ecs::world::World;
use fl_content::ContentDb;
use fl_protocol::Tick;
use fl_view::{
    JournalEntry, OpaqueContent, RunnerControl, SimQuery, SimReply, TickDriver, TickInput,
    TickOutput,
};
use serde_json::{Map, Value, json};

use crate::build::MatchHeaderRes;
use crate::clock::SimClock;
use crate::counters::SimCounters;
use crate::debug_query::debug_query;
use crate::inspect::{inspect_entity, inspect_world};
use crate::reload::apply_content_values;
use crate::save::save_world;
use crate::step::step_tick;

pub use lockstep::{LockstepDriver, LoopbackSession, NetSession, loopback};
pub use replay_driver::ReplayDriver;
pub use test_driver::TestDriver;

/// How many of the latest journal entries a [`SimRunner`] keeps for
/// [`SimQuery::JournalSince`].
pub const JOURNAL_RING: usize = 10_000;

/// Owns a sim `World` built by [`build_sim_app`](crate::build_sim_app) and steps it with
/// [`step_tick`]. Same API inline and, after [`SimRunner::spawn_threaded`], over channels.
pub struct SimRunner {
    world: World,
    /// The latest [`JOURNAL_RING`] journal entries, oldest first.
    journal: VecDeque<JournalEntry>,
}

impl SimRunner {
    /// Take over the sim of `app` (its `World`, which holds the `SimTick` schedule).
    pub fn new(mut app: App) -> SimRunner {
        SimRunner {
            world: std::mem::take(app.world_mut()),
            journal: VecDeque::new(),
        }
    }

    /// The tick the next [`SimRunner::step`] runs.
    pub fn next_tick(&self) -> Tick {
        self.world.resource::<SimClock>().tick
    }

    /// Run one tick; see [`step_tick`].
    ///
    /// # Panics
    /// As [`step_tick`] does: `input.tick` is not [`SimRunner::next_tick`], or a sim system
    /// panicked.
    pub fn step(&mut self, input: TickInput) -> TickOutput {
        let output = step_tick(&mut self.world, input);
        self.journal.extend(output.journal.iter().cloned());
        let excess = self.journal.len().saturating_sub(JOURNAL_RING);
        self.journal.drain(..excess);
        output
    }

    /// Run `ticks` ticks with the inputs `driver` hands out, returning every output.
    ///
    /// # Panics
    /// `"driver stalled at tick T"` if the driver has no input ready, and as
    /// [`SimRunner::step`] does.
    pub fn run(&mut self, driver: &mut dyn TickDriver, ticks: u32) -> Vec<TickOutput> {
        (0..ticks)
            .map(|_| {
                let input = driver
                    .poll()
                    .unwrap_or_else(|| panic!("driver stalled at tick {}", driver.next_tick().0));
                self.step(input)
            })
            .collect()
    }

    /// Answer `q` from the state between ticks:
    /// - `Entity` → [`inspect_entity`]; `WorldSummary` → [`inspect_world`];
    /// - `Perf` → `{"counters": {name: n}}`, the [`SimCounters`] of the last tick;
    /// - `Header` → the match header;
    /// - `JournalSince { tick, limit }` → the first `limit` of the latest [`JOURNAL_RING`]
    ///   journal entries with a tick of at least `tick`, in journal order;
    /// - `Save { pending }` → [`save_world`] with the driver's `pending` commands, or
    ///   [`SimReply::Error`] when the world cannot be saved now (the match runs on);
    /// - `Debug { name, args }` → [`debug_query`]: `SimReply::Debug(answer)`, or
    ///   [`SimReply::Error`] with `"unknown debug query '<name>'"` or the query's own message.
    ///
    /// # Panics
    /// As [`save_world`] does (a codec that cannot encode its value).
    pub fn query(&mut self, q: SimQuery) -> SimReply {
        let world = &self.world;
        match q {
            SimQuery::Entity(id) => SimReply::Entity(inspect_entity(world, id)),
            SimQuery::WorldSummary => SimReply::WorldSummary(inspect_world(world)),
            SimQuery::Perf => {
                let counters: Map<String, Value> = world
                    .resource::<SimCounters>()
                    .snapshot()
                    .into_iter()
                    .map(|(name, n)| (name.to_owned(), Value::from(n)))
                    .collect();
                SimReply::Perf(json!({ "counters": counters }))
            }
            SimQuery::Header => SimReply::Header(world.resource::<MatchHeaderRes>().0.clone()),
            SimQuery::JournalSince { tick, limit } => SimReply::Journal(
                self.journal
                    .iter()
                    .filter(|entry| entry.tick >= tick)
                    .take(usize::try_from(limit).unwrap_or(usize::MAX))
                    .cloned()
                    .collect(),
            ),
            SimQuery::Save { pending } => match save_world(world, &pending) {
                Ok(bytes) => SimReply::Save(bytes),
                Err(err) => SimReply::Error(err.to_string()),
            },
            SimQuery::Debug { name, args } => match debug_query(world, &name, &args) {
                Ok(answer) => SimReply::Debug(answer),
                Err(message) => SimReply::Error(message),
            },
            other => SimReply::Error(format!("unsupported query: {other:?}")),
        }
    }

    /// Carry out `control` between ticks:
    /// - `ReloadContent(content)` → [`apply_content_values`] with `content` downcast to a
    ///   [`ContentDb`]: [`SimReply::Reloaded`] with the tick the match runs on with and the new
    ///   rules hash, or [`SimReply::Error`] with the refusal (the match is unchanged) or
    ///   `"the reload content is not a ContentDb"`.
    pub fn control(&mut self, control: RunnerControl) -> SimReply {
        match control {
            RunnerControl::ReloadContent(OpaqueContent(content)) => {
                let Ok(content) = content.downcast::<ContentDb>() else {
                    return SimReply::Error("the reload content is not a ContentDb".to_owned());
                };
                match apply_content_values(&mut self.world, content) {
                    Ok(report) => SimReply::Reloaded {
                        tick: report.tick,
                        rules_hash: report.rules_hash,
                    },
                    Err(err) => SimReply::Error(err.to_string()),
                }
            }
            other => SimReply::Error(format!("unsupported control: {other:?}")),
        }
    }

    /// The sim world, for reads.
    pub fn world(&self) -> &World {
        &self.world
    }

    /// The sim world, for setup and tests only: the sim changes through commands.
    pub fn world_mut(&mut self) -> &mut World {
        &mut self.world
    }
}
