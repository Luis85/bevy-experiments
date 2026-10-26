//! Replay capture of the running match (ADR-0012 refinement): the console command
//! `replay save <path>` writes the match as a [`Replay`], so hot reloads have a replay to
//! record their `ContentReload` events in.
//!
//! The capture keeps the match's header, its [`ReplayStart`] and hash interval, and the
//! reload events; the client's [`InputLog`] has every input sent and every hash received. A
//! loaded scenario or save starts a new capture; a hot-reload rebuild continues it. A replay
//! with a reload event is not reproducible, and `replay-verify` reports it as such.

use bevy::prelude::*;
use fl_client::debug::ConsoleAppExt;
use fl_client::link::InputLog;
use fl_protocol::MatchHeader;
use fl_sim::{
    HashInterval, MatchHeaderRes, REPLAY_FORMAT_VERSION, Replay, ReplayEvent, ReplayStart,
};

/// The replay of the running match, without its inputs (those are the [`InputLog`]'s).
#[derive(Resource, Clone, Debug, PartialEq)]
pub struct ReplayCapture {
    header: MatchHeader,
    start: ReplayStart,
    hash_interval: u32,
    events: Vec<ReplayEvent>,
}

impl ReplayCapture {
    /// The capture of the match in the sim `world` (built by `build_sim_app` or
    /// `load_world`), set up by `start`.
    ///
    /// # Panics
    /// If `world` is not a sim world.
    #[must_use]
    pub fn of_sim(world: &World, start: ReplayStart) -> Self {
        ReplayCapture {
            header: world.resource::<MatchHeaderRes>().0.clone(),
            start,
            hash_interval: world.resource::<HashInterval>().0,
            events: Vec::new(),
        }
    }

    /// Appends an event of the match.
    pub fn push(&mut self, event: ReplayEvent) {
        self.events.push(event);
    }

    /// The events so far.
    #[must_use]
    pub fn events(&self) -> &[ReplayEvent] {
        &self.events
    }

    /// The replay of the match driven as `log` says: its ticks sent, its inputs with
    /// commands, every hash received.
    #[must_use]
    pub fn replay(&self, log: &InputLog) -> Replay {
        Replay {
            version: REPLAY_FORMAT_VERSION,
            header: self.header.clone(),
            start: self.start.clone(),
            hash_interval: self.hash_interval,
            ticks: log.ticks,
            inputs: log.inputs.clone(),
            hashes: log.hashes.clone(),
            events: self.events.clone(),
            semantic: Vec::new(),
            journal_summary: std::collections::BTreeMap::new(),
        }
    }
}

/// The console command `replay save <path>`.
fn replay_command(world: &mut World, args: &[&str]) -> Result<String, String> {
    let ["save", path] = args else {
        return Err("usage: replay save <path>".to_owned());
    };
    let (Some(capture), Some(log)) = (
        world.get_resource::<ReplayCapture>(),
        world.get_resource::<InputLog>(),
    ) else {
        return Err("this match is not being captured".to_owned());
    };
    let replay = capture.replay(log);
    let bytes = replay.to_bytes();
    std::fs::write(path, &bytes).map_err(|err| format!("cannot write {path}: {err}"))?;
    let note = if replay.reproducible() {
        ""
    } else {
        "; it has a content reload, so it is not reproducible"
    };
    Ok(format!(
        "replay of {} ticks written to {path} ({} bytes){note}",
        replay.ticks,
        bytes.len()
    ))
}

/// The `replay` console command and the [`InputLog`].
pub(crate) fn build(app: &mut App) {
    app.init_resource::<InputLog>().register_console_command(
        "replay",
        "replay save <path>: write a replay of the match",
        replay_command,
    );
}
