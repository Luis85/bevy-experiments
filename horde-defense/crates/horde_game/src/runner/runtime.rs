//! The `Runtime` resource: command queue, one-shot controls, save bookkeeping,
//! tick execution and the sound cues of drained events.
use super::{command::*, persistence};
use crate::audio::Cue as SoundCue;
use bevy::prelude::*;
#[cfg(feature = "dev")]
use horde_core::command::Command;
use horde_core::{
    content::{Mode, StructureKind},
    ordnance::OrdnanceKind,
    sim::{Actor, BattleId, Game, Input, Outcome, SimEvent},
};
#[cfg(feature = "review")]
use persistence::PersistenceMetrics;
use std::collections::VecDeque;

/// The authoritative game. Only the runner mutates it (through `Game::apply`
/// and fixed ticks); rendering reads it, so its change ticks mean real changes.
#[derive(Resource)]
pub struct GameSession {
    pub game: Game,
}
#[derive(Clone, Debug)]
pub(super) struct QueuedCommand {
    pub(super) command: AppCommand,
    pub(super) success: String,
}
#[derive(Resource, Debug, Default)]
pub struct Runtime {
    pub(super) frame: Option<u32>,
    pub(super) seen: Vec<AppCommand>,
    pub(super) queue: VecDeque<QueuedCommand>,
    pub(super) controls: ControlFrame,
    pub(super) pending: Input,
    pub(super) pending_aims: AttackAims,
    pub(super) save_requests: Vec<SaveIntent>,
    pub(super) observed_battle: Option<u64>,
    pub(super) dirty: bool,
    pub(super) save_ticks: u64,
    pub(super) retry_save_at: u64,
    pub(super) storage: persistence::Service,
    pub(super) reset_success: Option<String>,
    pub fixed_calls: u64,
    pub simulated_ticks: u64,
    /// `dev`: command/input recording in progress.
    #[cfg(feature = "dev")]
    pub(super) recorder: Option<crate::debug_tools::recording::Recorder>,
    /// `dev`: pending single-tick requests (F5), consumed in `FixedUpdate`.
    #[cfg(feature = "dev")]
    pub(super) step_requests: u32,
    /// `dev`: newest root event sequence number seen, and settlement failures so far.
    #[cfg(feature = "dev")]
    pub(super) last_event_seq: Option<u64>,
    #[cfg(feature = "dev")]
    pub(super) settlement_issue_total: usize,
}

impl Runtime {
    /// Called on each egui pass with the native FrameCount. Repeated layout
    /// passes share a command identity set, preventing duplicate purchases.
    pub fn durable_transition_pending(&self) -> bool {
        self.storage.blocks_gameplay()
            || self.save_requests.iter().any(|i| {
                matches!(
                    i,
                    SaveIntent::Menu | SaveIntent::Quit | SaveIntent::SettingsBack
                )
            })
            || self.queue.iter().any(|q| {
                matches!(
                    q.command,
                    AppCommand::NewJourney
                        | AppCommand::Save(
                            SaveIntent::Menu | SaveIntent::Quit | SaveIntent::SettingsBack
                        )
                )
            })
    }
    pub fn user_save_pending(&self) -> bool {
        self.storage.user_pending()
            || self
                .save_requests
                .iter()
                .any(|i| *i != SaveIntent::Autosave)
            || self.queue.iter().any(|q| {
                matches!(q.command, AppCommand::NewJourney)
                    || matches!(q.command,AppCommand::Save(intent) if intent!=SaveIntent::Autosave)
            })
    }
    #[cfg(test)]
    pub(crate) fn gated_storage_for_test(&mut self) -> persistence::Gate {
        let (service, gate) = persistence::gated();
        self.storage = service;
        gate
    }
    #[cfg(feature = "review")]
    pub fn persistence_metrics(&self) -> PersistenceMetrics {
        let mut metrics = self.storage.metrics();
        metrics.pending |= !self.save_requests.is_empty();
        metrics
    }
    #[cfg(test)]
    pub(crate) fn finish_storage_for_test(&mut self) {
        drop(std::mem::take(&mut self.storage));
    }
    pub fn reset_in_progress(&self) -> bool {
        self.storage.reset_pending()
    }
    pub fn begin_frame(&mut self, frame: u32) {
        if self.frame != Some(frame) {
            self.frame = Some(frame);
            self.seen.clear();
        }
    }
    pub fn enqueue(&mut self, command: impl Into<AppCommand>, success: impl Into<String>) -> bool {
        let command = command.into();
        if matches!(command, AppCommand::NewJourney) && self.reset_in_progress() {
            return false;
        }
        if self.durable_transition_pending()
            && !matches!(command, AppCommand::Save(_) | AppCommand::NewJourney)
        {
            return false;
        }

        if self.queue.len() >= 256 || self.seen.contains(&command) {
            return false;
        }
        self.seen.push(command.clone());
        self.queue.push_back(QueuedCommand {
            command,
            success: success.into(),
        });
        true
    }
    pub fn publish(&mut self, controls: ControlFrame) {
        if controls.enabled {
            if controls.pressed.fire && !self.pending.fire {
                self.pending_aims.fire =
                    Some(controls.pressed_aims.fire.unwrap_or(controls.pressed.aim));
            }
            if controls.pressed.melee && !self.pending.melee {
                self.pending_aims.melee =
                    Some(controls.pressed_aims.melee.unwrap_or(controls.pressed.aim));
            }
            merge_actions(&mut self.pending, &controls.pressed);
        } else {
            self.pending = Input::default();
            self.pending_aims = AttackAims::default();
        }
        self.controls = controls;
        self.controls.speed = match self.controls.speed {
            2 => 2,
            4 => 4,
            _ => 1,
        };
    }
    /// The expedition the camp is watching, published by the UI each frame;
    /// it hears its ordinary cues in full.
    pub fn observe_battle(&mut self, battle: Option<u64>) {
        self.observed_battle = battle;
    }
    pub fn request_save(&mut self, intent: SaveIntent) {
        if !self.save_requests.contains(&intent) {
            self.save_requests.push(intent);
        }
    }
    pub fn clear_input(&mut self) {
        self.pending = Input::default();
        self.pending_aims = AttackAims::default();
        self.controls.held = Input::default();
    }
    pub fn pending_commands(&self) -> usize {
        self.queue.len()
    }
    pub(super) fn input(&mut self) -> Input {
        let mut input = self.controls.held.clone();
        input.throw = None;
        // Coincident attacks keep the core's melee-first action order. The
        // earliest unconsumed edge of each action owns its original direction.
        if let Some(aim) = self.pending_aims.melee.or(self.pending_aims.fire) {
            input.aim = aim;
        }
        merge_actions(&mut input, &self.pending);
        self.pending = Input::default();
        self.pending_aims = AttackAims::default();
        input
    }
    pub(super) fn advance(&mut self, game: &mut Game, cues: &mut Vec<SoundCue>) -> bool {
        let _work = crate::diagnostics::work_timer(crate::diagnostics::WorkPhase::Simulation);
        self.fixed_calls += 1;
        #[cfg(feature = "dev")]
        if self.step_requests > 0 {
            self.step_requests -= 1;
            // A request that cannot act is dropped and the ordinary tick follows.
            if let Some(finished) = self.step_once(game, cues) {
                return finished;
            }
        }
        if !self.controls.enabled
            || self.durable_transition_pending()
            || game.global_paused()
            || game.attempt().is_some_and(|a| a.paused)
        {
            self.clear_input();
            return false;
        }
        for _ in 0..self.controls.speed {
            if self.run_tick(game, cues) {
                return true;
            }
        }
        false
    }
    /// One authoritative tick with the current input. True when it finished the
    /// attempt, which pauses the game and disables further controls.
    pub(super) fn run_tick(&mut self, game: &mut Game, cues: &mut Vec<SoundCue>) -> bool {
        let input = self.input();
        #[cfg(feature = "dev")]
        if let Some(recorder) = &mut self.recorder {
            recorder.input(self.simulated_ticks, &input);
        }
        let had_attempt = game.attempt().is_some();
        game.tick(input);
        self.simulated_ticks += 1;
        self.save_ticks += 1;
        self.drain_sounds(game, cues);
        if self.save_ticks >= 600 {
            self.dirty = true;
        }
        if had_attempt && game.attempt().is_none() {
            self.dirty = true;
            self.clear_input();
            game.set_paused(true);
            self.controls.enabled = false;
            return true;
        }
        false
    }
    /// Turns the game's undrained events into cues now. Ticks call it, and so
    /// does a command that finishes the attempt outside a tick (`Withdraw`
    /// pauses the game, so its terminal event would otherwise wait).
    pub(super) fn drain_sounds(&mut self, game: &mut Game, cues: &mut Vec<SoundCue>) {
        let newest_event = tick_sounds(game, self.observed_battle, cues);
        #[cfg(feature = "dev")]
        {
            self.last_event_seq = newest_event.or(self.last_event_seq);
        }
        #[cfg(not(feature = "dev"))]
        let _ = newest_event;
    }
}
/// Runner state shown by the F3 panel.
#[cfg(feature = "dev")]
#[derive(Clone, Copy, Debug)]
pub(crate) struct RuntimeDebug {
    pub speed: u32,
    pub controls_enabled: bool,
    pub queued_commands: usize,
    pub save_requests: usize,
    pub durable_transition: bool,
    pub user_save_pending: bool,
    pub reset_in_progress: bool,
    pub simulated_ticks: u64,
    pub last_event_seq: Option<u64>,
    pub settlement_issue_total: usize,
    pub step_requests: u32,
}
/// `dev` hooks: recording and single-tick stepping.
#[cfg(feature = "dev")]
impl Runtime {
    pub(crate) fn start_recording(&mut self, recorder: crate::debug_tools::recording::Recorder) {
        self.recorder = Some(recorder);
    }
    /// Ends and flushes the recording; true when one was running.
    pub(crate) fn stop_recording(&mut self, game: &Game, reason: &str) -> bool {
        let Some(recorder) = self.recorder.take() else {
            return false;
        };
        recorder.finish(self.simulated_ticks, game, reason);
        true
    }
    /// What the F3 panel shows about the runner.
    pub(crate) fn debug_snapshot(&self) -> RuntimeDebug {
        RuntimeDebug {
            speed: self.controls.speed,
            controls_enabled: self.controls.enabled,
            queued_commands: self.queue.len(),
            save_requests: self.save_requests.len(),
            durable_transition: self.durable_transition_pending(),
            user_save_pending: self.user_save_pending(),
            reset_in_progress: self.reset_in_progress(),
            simulated_ticks: self.simulated_ticks,
            last_event_seq: self.last_event_seq,
            settlement_issue_total: self.settlement_issue_total,
            step_requests: self.step_requests,
        }
    }
    pub(crate) fn recording_path(&self) -> Option<&std::path::Path> {
        self.recorder.as_ref().map(|r| r.path())
    }
    /// Pushes buffered recording lines to the file; called once per frame.
    pub(crate) fn flush_recording(&mut self) {
        if let Some(recorder) = &mut self.recorder
            && let Err(error) = recorder.flush()
        {
            warn!("Recording stopped: {error}");
            self.recorder = None;
        }
    }
    pub(super) fn record_command(&mut self, command: &Command) {
        if let Some(recorder) = &mut self.recorder {
            recorder.command(self.simulated_ticks, command);
        }
    }
    /// F5: asks the next fixed pass for one tick even while paused.
    pub(crate) fn request_step(&mut self) {
        self.step_requests = (self.step_requests + 1).min(8);
    }
    /// One tick of a paused game: unpause, tick, re-pause, all through
    /// `Game::apply` so a recording replays it. `None` (nothing consumed, no
    /// tick run) while the game is running, has no attempt, or a durable
    /// transition is pending; otherwise `Some(attempt finished)`.
    pub(super) fn step_once(&mut self, game: &mut Game, cues: &mut Vec<SoundCue>) -> Option<bool> {
        let paused = game.global_paused() || game.attempt().is_some_and(|a| a.paused);
        if !paused || game.attempt().is_none() || self.durable_transition_pending() {
            return None;
        }
        let unpause = Command::SetPaused(false);
        self.record_command(&unpause);
        let _ = game.apply(&unpause);
        let finished = self.run_tick(game, cues);
        if !finished {
            let command = Command::SetPaused(true);
            self.record_command(&command);
            let _ = game.apply(&command);
        }
        Some(finished)
    }
}
pub(super) fn merge_actions(into: &mut Input, from: &Input) {
    into.fire |= from.fire;
    into.melee |= from.melee;
    into.reload |= from.reload;
    into.heal |= from.heal;
    into.brace |= from.brace;
    if into.throw.is_none() {
        into.throw = from.throw;
    }
}

/// Queues `cue` once per batch.
pub(crate) fn push_cue(cues: &mut Vec<SoundCue>, cue: SoundCue) {
    if !cues.contains(&cue) {
        cues.push(cue);
    }
}
pub(super) fn event_sound(cues: &mut Vec<SoundCue>, event: &SimEvent) {
    if let SimEvent::Hit(strike)
    | SimEvent::Armor(strike)
    | SimEvent::Miss(strike)
    | SimEvent::Defended(strike) = event
        && matches!(
            strike.attacker,
            Actor::Structure {
                kind: StructureKind::Trap,
                ..
            }
        )
    {
        push_cue(cues, SoundCue::Trap);
    }
    let cue = match event {
        SimEvent::Boss { .. } => Some(SoundCue::Boss),
        SimEvent::BossWarning { .. } | SimEvent::Reinforcements { .. } => Some(SoundCue::Wave),
        SimEvent::BossExposure { .. } | SimEvent::CooldownReady => Some(SoundCue::Ready),
        SimEvent::StructureLost { .. } => Some(SoundCue::Structure),
        SimEvent::Damage(_) => Some(SoundCue::Damage),
        SimEvent::Melee(_) => Some(SoundCue::Melee),
        SimEvent::Reload => Some(SoundCue::Reload),
        SimEvent::Shot { shooter, .. } => match *shooter {
            Actor::Structure {
                kind: StructureKind::Trap,
                ..
            } => Some(SoundCue::Trap),
            Actor::Structure {
                kind: StructureKind::Turret,
                ..
            } => Some(SoundCue::TurretShot),
            Actor::Defender {
                mode: Mode::Npc, ..
            } => Some(SoundCue::NpcShot),
            Actor::Defender { weapon, .. } => Some(match weapon {
                horde_core::weapons::WeaponClass::Pistol => SoundCue::Pistol,
                horde_core::weapons::WeaponClass::Smg => SoundCue::Smg,
                horde_core::weapons::WeaponClass::Shotgun => SoundCue::Shotgun,
                horde_core::weapons::WeaponClass::Rifle => SoundCue::Rifle,
                horde_core::weapons::WeaponClass::Marksman => SoundCue::Marksman,
                horde_core::weapons::WeaponClass::MachineGun => SoundCue::MachineGun,
            }),
            // A source that no longer exists has no reliable timbre.
            _ => None,
        },
        SimEvent::Throw { .. } => Some(SoundCue::Throw),
        SimEvent::Explosion {
            ordnance: OrdnanceKind::Molotov,
            ..
        } => Some(SoundCue::Molotov),
        SimEvent::Explosion { .. } => Some(SoundCue::Explosion),
        SimEvent::Burn { .. } => Some(SoundCue::Fire),
        SimEvent::Hit(_) | SimEvent::WeakPoint(_) => Some(SoundCue::Impact),
        SimEvent::Armor(_) => Some(SoundCue::Armor),
        SimEvent::Spawn { .. } => Some(SoundCue::Incoming),
        SimEvent::Kill { .. } => Some(SoundCue::Kill),
        SimEvent::AttemptFinished { outcome, .. } => Some(match outcome {
            Outcome::Victory => SoundCue::Victory,
            Outcome::Defeat => SoundCue::Defeat,
            Outcome::Withdrawal => SoundCue::Retreat,
        }),
        _ => None,
    };
    if let Some(cue) = cue {
        push_cue(cues, cue);
    }
}
/// Sounds for one tick's drained events. The main deployment is always
/// audible; camp observation hears the observed expedition in full and only
/// important cues from the others, without multiplying ordinary shots.
/// Returns the newest root event sequence number it drained.
pub(super) fn tick_sounds(
    game: &mut Game,
    observed: Option<u64>,
    cues: &mut Vec<SoundCue>,
) -> Option<u64> {
    let camp = game.attempt().is_none();
    let mut newest = None;
    for record in game.drain_events() {
        if record.battle == BattleId::Root {
            newest = Some(record.seq);
        }
        let audible = match record.battle {
            BattleId::Root => true,
            BattleId::Expedition(id) => {
                camp && (observed == Some(id)
                    || matches!(
                        record.event,
                        SimEvent::Boss { .. }
                            | SimEvent::BossWarning { .. }
                            | SimEvent::Reinforcements { .. }
                            | SimEvent::StructureLost { .. }
                    ))
            }
        };
        if audible {
            event_sound(cues, &record.event);
        }
    }
    let fire = match game.attempt() {
        Some(a) => !a.ordnance.fire_zones.is_empty(),
        None => game.battles().iter().any(|battle| {
            !battle.settled
                && observed == Some(battle.id)
                && battle
                    .game()
                    .attempt()
                    .is_some_and(|a| !a.ordnance.fire_zones.is_empty())
        }),
    };
    if fire {
        push_cue(cues, SoundCue::Fire);
    }
    newest
}
