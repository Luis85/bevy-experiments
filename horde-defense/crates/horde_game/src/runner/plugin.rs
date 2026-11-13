//! Bevy wiring: the runner plugin, its system sets and systems.
use super::{
    apply::{RuntimeCtx, flush, persist_pending, step},
    command::RuntimeNotice,
    runtime::{GameSession, Runtime},
};
use crate::{audio::CueRequest, profile::ProfilePaths, settings::Settings};
use bevy::{app::AppExit, ecs::system::SystemParam, prelude::*};

/// Runner phases. Only `Apply` has dependants (the UI's notice handling runs
/// after it); the phases are in different schedules, so no chain is needed.
#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RuntimeSet {
    /// PreUpdate: queued commands reach `Game::apply`; finished saves are polled.
    Apply,
    /// FixedUpdate (60 Hz): whole authoritative ticks.
    Advance,
    /// Update: storage acknowledgements and autosave submission.
    Persist,
}

/// Owns the fixed clock and the command/tick/persist services over
/// `GameSession` and `Runtime`.
pub struct SimRuntimePlugin;
impl Plugin for SimRuntimePlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Time::<Fixed>::from_hz(60.0))
            .add_message::<RuntimeNotice>()
            .add_message::<CueRequest>()
            .add_systems(PreUpdate, apply_commands.in_set(RuntimeSet::Apply))
            .add_systems(FixedUpdate, advance.in_set(RuntimeSet::Advance))
            .add_systems(Update, persist.in_set(RuntimeSet::Persist));
    }
}

/// The resources one runner pass reads and writes.
#[derive(SystemParam)]
pub struct RuntimeParams<'w> {
    session: ResMut<'w, GameSession>,
    runtime: ResMut<'w, Runtime>,
    settings: Res<'w, Settings>,
    paths: Res<'w, ProfilePaths>,
    notices: MessageWriter<'w, RuntimeNotice>,
    cues: MessageWriter<'w, CueRequest>,
    perf: Option<Res<'w, crate::diagnostics::PerfProbe>>,
}
impl RuntimeParams<'_> {
    fn run<R>(&mut self, pass: impl FnOnce(&mut RuntimeCtx) -> R) -> R {
        let mut notices = Vec::new();
        let mut cues = Vec::new();
        let (result, changed) = {
            let mut ctx = RuntimeCtx {
                // Marked changed below only when the pass really mutated it.
                game: &mut self.session.bypass_change_detection().game,
                runtime: &mut self.runtime,
                settings: &self.settings,
                paths: &self.paths,
                notices: &mut notices,
                cues: &mut cues,
                game_changed: false,
                perf: self.perf.as_deref(),
            };
            let result = pass(&mut ctx);
            (result, ctx.game_changed)
        };
        if changed {
            self.session.set_changed();
        }
        self.notices.write_batch(notices);
        self.cues.write_batch(cues.into_iter().map(CueRequest));
        result
    }
}
fn apply_commands(mut params: RuntimeParams, mut exit: MessageWriter<AppExit>) {
    if params.run(flush) {
        exit.write(AppExit::Success);
    }
}
fn advance(mut params: RuntimeParams) {
    params.run(step);
}
fn persist(mut params: RuntimeParams, mut exit: MessageWriter<AppExit>) {
    if params.run(persist_pending) {
        exit.write(AppExit::Success);
    }
}
