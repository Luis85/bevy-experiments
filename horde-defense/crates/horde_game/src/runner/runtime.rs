//! The `Runtime` resource: command queue, one-shot controls, save bookkeeping,
//! tick execution and the sound cues of drained events.
use super::{command::*, persistence};
use crate::audio::Cue as SoundCue;
use bevy::prelude::*;
pub(crate) use horde_audio::events::push_cue;
#[cfg(feature = "dev")]
use horde_core::command::Command;
// The event-to-cue mapping lives in `horde_audio`, shared with the offline
// renderer.
#[cfg(test)]
pub(super) use horde_audio::events::event_sound;
use horde_audio::{
    events::{EventContext, event_requests},
    gun::GunTracker,
    loot::{ExpeditionCoins, LootTracker},
    pan::{self, Places},
    render::fire_bed_x,
    voicing::EnemyVoiceIndex,
};
use horde_core::sim::{BattleId, Game, Input, SimEvent};
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
    /// Lane pan places of the cues drained since the runner last forwarded
    /// its cues (`RuntimeParams::run` takes them).
    pub(super) cue_places: Places,
    /// The voices of the live enemies (`horde_audio::voicing`), synced
    /// around every tick. Presentation only: never saved, rebuilt from the
    /// attempt after a load.
    pub(super) enemy_voices: EnemyVoiceIndex,
    /// The root defender's gun (`horde_audio::gun`): reload stages, shell
    /// inserts, the bolt and the dry trigger, heard after every tick with
    /// that tick's input. Presentation only: never saved; it primes on an
    /// attempt it has not seen, so a load plays no stage burst. An
    /// expedition watched from camp has no gun handling.
    pub(super) guns: GunTracker,
    /// The root battle's drops (`horde_audio::loot`): a Rare or better drop
    /// plays `LootDrop` where it fell. Presentation only: never saved; it
    /// primes on an attempt it has not seen, so a load replays no drop. No
    /// widget plays a loot sound.
    pub(super) loot: LootTracker,
    /// The settled expeditions whose auto-sold batch has been heard: one
    /// `CoinBatch` each, never during a battle. Presentation only.
    pub(super) expedition_coins: ExpeditionCoins,
    pub(super) dirty: bool,
    pub(super) save_ticks: u64,
    pub(super) retry_save_at: u64,
    pub(super) storage: persistence::Service,
    pub(super) reset_success: Option<String>,
    /// The startup read left `settings.json` uncopied: every save copies it
    /// first, until the worker has done so once.
    pub(super) settings_at_risk: bool,
    /// Review-only: the staged game can never be saved (the stress fixture),
    /// so no save is ever submitted. See [`Runtime::forbid_saves`].
    pub(super) unsaveable: bool,
    /// What `AppCommand::OpenSaveFolder` runs through.
    pub(super) folder_opener: super::folder::FolderOpenerSlot,
    /// A gameplay command other than a pause changed the game since startup
    /// (or since the last successful `ReloadProfile`).
    pub(super) commands_applied: bool,
    /// A save or a journey reset was handed to storage since startup, so the
    /// profile files may no longer be the ones the startup load read.
    pub(super) saved: bool,
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
    /// Startup could not preserve `settings.json` (`LoadedProfile::settings_at_risk`).
    pub fn guard_settings_original(&mut self) {
        self.settings_at_risk = true;
    }
    /// Review-only, for a staged game production validation rejects (the
    /// stress scene): the periodic autosave never fires and no intent reaches
    /// storage, so no save can fail. A Quit still closes (a perf report ends
    /// with one); every other user intent finishes with
    /// `RuntimeNotice::SaveSkipped`, so its navigation runs without a save.
    /// The isolated review profile has nothing to keep.
    #[cfg(feature = "review")]
    pub fn forbid_saves(&mut self) {
        self.unsaveable = true;
    }
    /// Replaces the opener `AppCommand::OpenSaveFolder` runs through.
    #[cfg(test)]
    pub(crate) fn set_folder_opener(&mut self, opener: impl super::folder::FolderOpener) {
        self.folder_opener = super::folder::FolderOpenerSlot(Box::new(opener));
    }
    /// A gameplay command changed the in-memory journey since it was loaded.
    /// Pausing is not progress and does not count.
    pub fn commands_applied_since_load(&self) -> bool {
        self.commands_applied
    }
    /// A save or a journey reset reached storage since the journey was loaded.
    pub fn saved_since_load(&self) -> bool {
        self.saved
    }
    /// Whether `AppCommand::ReloadProfile` may replace the in-memory journey
    /// (design-ux-flow §3 "Runner"): nothing was played and nothing was saved
    /// since the load. After a save the unreadable file has been replaced
    /// (after its recovery copy), so there is nothing left to read again.
    pub fn reload_allowed(&self) -> bool {
        !self.commands_applied_since_load()
            && !self.saved_since_load()
            && self.simulated_ticks == 0
            && !self.unsaveable
            && !self.durable_transition_pending()
    }
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
    /// Called on each egui pass with the native FrameCount. Repeated layout
    /// passes share a command identity set, preventing duplicate purchases.
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
            && !matches!(
                command,
                AppCommand::Save(_) | AppCommand::NewJourney | AppCommand::OpenSaveFolder
            )
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
    pub(super) fn advance(
        &mut self,
        game: &mut Game,
        cues: &mut Vec<SoundCue>,
        perf: Option<&crate::diagnostics::PerfProbe>,
    ) -> bool {
        let _work = crate::diagnostics::work_timer(perf, crate::diagnostics::WorkPhase::Simulation);
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
        // Before the tick: the enemies this tick kills still have voices.
        sync_voices(game, self.observed_battle, &mut self.enemy_voices);
        game.tick(input.clone());
        self.simulated_ticks += 1;
        self.save_ticks += 1;
        self.drain_sounds(game, cues);
        // After the tick's events, so the tracker can tune the Reload the
        // core announced.
        match game.attempt() {
            Some(attempt) => {
                self.guns
                    .observe(BattleId::Root, attempt, &input, cues, &mut self.cue_places);
                self.loot
                    .observe(BattleId::Root, attempt, cues, &mut self.cue_places);
            }
            None => {
                self.guns.retain(|_| false);
                self.loot.retain(|_| false);
            }
        }
        self.hear_expedition_coins(game, cues);
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
    /// One `CoinBatch` when a settled expedition's receipt sold anything
    /// and its notice is new (`horde_audio::loot::ExpeditionCoins`). Ticks
    /// and commands call it; during a battle it waits.
    pub(super) fn hear_expedition_coins(&mut self, game: &Game, cues: &mut Vec<SoundCue>) {
        if self.expedition_coins.observe_game(game) > 0 {
            push_cue(cues, SoundCue::CoinBatch);
        }
    }
    /// Turns the game's undrained events into cues now. Ticks call it, and so
    /// does a command that finishes the attempt outside a tick (`Withdraw`
    /// pauses the game, so its terminal event would otherwise wait).
    pub(super) fn drain_sounds(&mut self, game: &mut Game, cues: &mut Vec<SoundCue>) {
        let newest_event = tick_sounds(
            game,
            self.observed_battle,
            cues,
            &mut self.cue_places,
            &mut self.enemy_voices,
        );
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

/// Brings `voices` up to the battles that can be heard in full: the root
/// attempt, and from camp the observed expedition. The voices of every other
/// battle are dropped.
pub(super) fn sync_voices(game: &Game, observed: Option<u64>, voices: &mut EnemyVoiceIndex) {
    let heard = if let Some(attempt) = game.attempt() {
        voices.sync(BattleId::Root, attempt);
        Some(BattleId::Root)
    } else {
        let battle = observed.and_then(|id| {
            game.battles()
                .iter()
                .find(|battle| battle.id == id && !battle.settled)
        });
        battle.and_then(|battle| {
            let attempt = battle.game().attempt()?;
            let key = BattleId::Expedition(battle.id);
            voices.sync(key, attempt);
            Some(key)
        })
    };
    voices.retain(|battle| Some(battle) == heard);
}

/// Sounds for one tick's drained events. The main deployment is always
/// audible; camp observation hears the observed expedition in full and only
/// important cues from the others, without multiplying ordinary shots.
/// The root battle's positional cues get their lane pan place in `places`;
/// an expedition heard from camp is centred and its shots are distant.
/// `voices` gives enemies their voices (`horde_audio::voicing`); it is
/// synced here again, for the enemies the tick spawned.
/// Returns the newest root event sequence number it drained.
pub(super) fn tick_sounds(
    game: &mut Game,
    observed: Option<u64>,
    cues: &mut Vec<SoundCue>,
    places: &mut Places,
    voices: &mut EnemyVoiceIndex,
) -> Option<u64> {
    let camp = game.attempt().is_none();
    let root_width = game.attempt().map(|a| a.width_yards);
    let mut newest = None;
    sync_voices(game, observed, voices);
    let records: Vec<_> = game.drain_events().collect();
    for record in records {
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
            // The root defender's own shot also takes the player-weapon lane.
            let ctx = match record.battle {
                BattleId::Root => EventContext {
                    battle: BattleId::Root,
                    width_yards: root_width,
                    distant: false,
                    // A same-tick spawn and kill resolves from its corpse.
                    corpses: game.attempt().map_or(&[], |a| a.corpses.as_slice()),
                },
                BattleId::Expedition(id) => EventContext {
                    battle: record.battle,
                    width_yards: None,
                    distant: true,
                    corpses: game
                        .battles()
                        .iter()
                        .find(|battle| battle.id == id)
                        .and_then(|battle| battle.game().attempt())
                        .map_or(&[], |a| a.corpses.as_slice()),
                },
            };
            event_requests(cues, places, voices, &record.event, &ctx);
        }
    }
    let fire = match game.attempt() {
        Some(a) => {
            if let Some(x) = fire_bed_x(&a.ordnance.fire_zones) {
                places.place(SoundCue::Fire, pan::lane_position(x, a.width_yards));
            }
            !a.ordnance.fire_zones.is_empty()
        }
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
