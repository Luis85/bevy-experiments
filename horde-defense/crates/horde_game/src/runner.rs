//! Native application orchestration. Rendering publishes intents; these services
//! apply commands, advance fixed simulation ticks, persist and consume audio.
//! No Bevy types cross the horde_core boundary.
use crate::settings::{self, Settings};
use bevy::{app::AppExit, prelude::*};
use horde_core::{
    command::{Command, CommandOutcome},
    content::{Mode, StructureKind},
    ordnance::OrdnanceKind,
    sim::{Actor, BattleId, Game, Input, SettlementIssue, SimEvent},
};
use std::collections::VecDeque;
use std::path::PathBuf;
mod persistence;
pub use persistence::PersistenceMetrics;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SaveIntent {
    Manual,
    Menu,
    Quit,
    SettingsBack,
    Autosave,
}
/// Queue entry published by the frontend: gameplay commands applied through
/// `Game::apply`, plus the durable reset and save intents the runner owns.
#[derive(Clone, Debug, PartialEq)]
pub enum AppCommand {
    /// Serialized storage barrier; never reaches `Game::apply`.
    NewJourney,
    Save(SaveIntent),
    Game(Command),
}
impl From<Command> for AppCommand {
    fn from(command: Command) -> Self {
        Self::Game(command)
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AppOutcome {
    Command(CommandOutcome),
    JourneyArchived(Option<PathBuf>),
}
#[derive(Clone, Debug)]
pub enum RuntimeNotice {
    CommandCompleted {
        command: AppCommand,
        result: Result<AppOutcome, String>,
        success: String,
    },
    Saved {
        intent: SaveIntent,
        result: Result<(), String>,
    },
    SimulationFinished,
    RecoveryCopies(Vec<PathBuf>),
    /// Expedition transfers that could not be applied to the shared campaign.
    SettlementIssues(Vec<SettlementIssue>),
}
/// Minimal bridge for the existing native frontend. Game remains an engine-free
/// aggregate; this facade can later be split into independent Bevy resources.
pub trait RuntimeHost: Resource<Mutability = bevy::ecs::component::Mutable> {
    fn runtime_parts(&mut self) -> (&mut Game, &mut Runtime);
    fn runtime_settings(&self) -> &Settings;
    fn runtime_paths(&self) -> (PathBuf, PathBuf);
    fn runtime_notice(&mut self, notice: RuntimeNotice);
    fn runtime_observed_battle(&self) -> Option<u64> {
        None
    }
}
#[derive(Clone, Copy, Debug, Default)]
pub struct AttackAims {
    pub fire: Option<[f32; 2]>,
    pub melee: Option<[f32; 2]>,
}
#[derive(Clone, Debug)]
pub struct ControlFrame {
    pub enabled: bool,
    pub speed: u32,
    pub held: Input,
    pub pressed: Input,
    pub pressed_aims: AttackAims,
}
impl Default for ControlFrame {
    fn default() -> Self {
        Self {
            enabled: false,
            speed: 1,
            held: Input::default(),
            pressed: Input::default(),
            pressed_aims: AttackAims::default(),
        }
    }
}
#[derive(Clone, Debug)]
struct QueuedCommand {
    command: AppCommand,
    success: String,
}
use crate::audio::Cue as SoundCue;
impl SoundCue {
    fn priority(self) -> u8 {
        match self {
            Self::Boss | Self::Retreat => 0,
            Self::Wave => 1,
            Self::Structure | Self::Explosion | Self::Molotov => 2,
            Self::Damage => 3,
            Self::Melee | Self::Throw => 4,
            Self::Reload => 5,
            Self::Ready => 6,
            Self::Armor | Self::Impact => 7,
            Self::Trap => 8,
            Self::Pistol
            | Self::Smg
            | Self::Shotgun
            | Self::Rifle
            | Self::Marksman
            | Self::MachineGun => 9,
            Self::NpcShot => 10,
            Self::TurretShot => 11,
            Self::Incoming => 12,
            Self::Fire => 13,
        }
    }
    fn fire(self) -> bool {
        matches!(
            self,
            Self::Pistol
                | Self::Smg
                | Self::Shotgun
                | Self::Rifle
                | Self::Marksman
                | Self::MachineGun
                | Self::NpcShot
                | Self::TurretShot
        )
    }
}
#[derive(Debug, Default)]
pub struct Runtime {
    frame: Option<u32>,
    seen: Vec<AppCommand>,
    queue: VecDeque<QueuedCommand>,
    controls: ControlFrame,
    pending: Input,
    pending_aims: AttackAims,
    save_requests: Vec<SaveIntent>,
    sounds: Vec<SoundCue>,
    audio_rotation: u64,
    cue_gate: crate::audio::CueGate,
    observed_battle: Option<u64>,
    dirty: bool,
    save_ticks: u64,
    retry_save_at: u64,
    storage: persistence::Service,
    reset_success: Option<String>,
    pub fixed_calls: u64,
    pub simulated_ticks: u64,
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
    pub fn notify_rare_loot(&mut self) {
        self.sound(SoundCue::Ready);
    }
    pub fn pending_commands(&self) -> usize {
        self.queue.len()
    }
    fn input(&mut self) -> Input {
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
    fn sound(&mut self, cue: SoundCue) {
        if !self.sounds.contains(&cue) {
            self.sounds.push(cue);
        }
    }
    fn event_sound(&mut self, event: &SimEvent) {
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
            self.sound(SoundCue::Trap);
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
            _ => None,
        };
        if let Some(cue) = cue {
            self.sound(cue);
        }
    }
    /// Sounds for one tick's drained events. The main deployment is always
    /// audible; camp observation hears the observed expedition in full and only
    /// important cues from the others, without multiplying ordinary shots.
    fn tick_sounds(&mut self, game: &mut Game) {
        let observed = self.observed_battle;
        let camp = game.attempt.is_none();
        for record in game.drain_events() {
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
                self.event_sound(&record.event);
            }
        }
        let fire = match &game.attempt {
            Some(a) => !a.ordnance.fire_zones.is_empty(),
            None => game.battles.iter().any(|battle| {
                !battle.settled
                    && observed == Some(battle.id)
                    && battle
                        .game
                        .attempt
                        .as_ref()
                        .is_some_and(|a| !a.ordnance.fire_zones.is_empty())
            }),
        };
        if fire {
            self.sound(SoundCue::Fire);
        }
    }
    fn advance(&mut self, game: &mut Game) -> bool {
        let _work = crate::diagnostics::work_timer(crate::diagnostics::WorkPhase::Simulation);
        self.fixed_calls += 1;
        if !self.controls.enabled
            || self.durable_transition_pending()
            || game.global_paused
            || game.attempt.as_ref().is_some_and(|a| a.paused)
        {
            self.clear_input();
            return false;
        }
        for _ in 0..self.controls.speed {
            let input = self.input();
            let had_attempt = game.attempt.is_some();
            game.tick(input);
            self.simulated_ticks += 1;
            self.save_ticks += 1;
            self.tick_sounds(game);
            if self.save_ticks >= 600 {
                self.dirty = true;
            }
            if had_attempt && game.attempt.is_none() {
                self.dirty = true;
                self.clear_input();
                game.set_paused(true);
                self.controls.enabled = false;
                return true;
            }
        }
        false
    }
}
fn merge_actions(into: &mut Input, from: &Input) {
    into.fire |= from.fire;
    into.melee |= from.melee;
    into.reload |= from.reload;
    into.heal |= from.heal;
    into.brace |= from.brace;
    if into.throw.is_none() {
        into.throw = from.throw;
    }
}

/// Pure application service: applies pending typed commands and durable saves.
/// Returns true only when a requested Quit has committed successfully.
pub fn flush<H: RuntimeHost>(host: &mut H) -> bool {
    let commands = {
        let (_, runtime) = host.runtime_parts();
        std::mem::take(&mut runtime.queue)
    };
    let mut commands = commands.into_iter();
    while let Some(queued) = commands.next() {
        // Save and reset intents are durable runner concerns; only gameplay
        // commands reach `Game::apply`.
        let command = match queued.command {
            AppCommand::Game(command) => command,
            AppCommand::Save(intent) => {
                host.runtime_parts().1.request_save(intent);
                continue;
            }
            AppCommand::NewJourney => {
                if let Err(error) = begin_new_journey(host, queued.success.clone()) {
                    host.runtime_notice(RuntimeNotice::CommandCompleted {
                        command: AppCommand::NewJourney,
                        result: Err(error),
                        success: queued.success,
                    });
                }
                // Later save/close intents belong to the confirmed new epoch.
                // They must survive this barrier and be recaptured after reset.
                for trailing in commands.by_ref() {
                    if let AppCommand::Save(intent) = trailing.command {
                        host.runtime_parts().1.request_save(intent);
                    } else {
                        host.runtime_notice(RuntimeNotice::CommandCompleted {
                            command: trailing.command,
                            result: Err("Command superseded by the new journey transition".into()),
                            success: trailing.success,
                        });
                    }
                }
                if !host.runtime_parts().1.reset_in_progress() {
                    requeue_deferred_saves(host);
                }
                break;
            }
        };
        if host.runtime_parts().1.durable_transition_pending() {
            host.runtime_notice(RuntimeNotice::CommandCompleted {
                command: AppCommand::Game(command),
                result: Err("Wait for the current durable transition to finish".into()),
                success: queued.success,
            });
            continue;
        }
        let result = {
            let (game, runtime) = host.runtime_parts();
            let result = game.apply(&command);
            if result.is_ok() {
                runtime.dirty = true;
                if matches!(command, Command::LeaveJourney) {
                    runtime.request_save(SaveIntent::Menu);
                }
                if matches!(command, Command::Withdraw) {
                    runtime.sound(SoundCue::Retreat);
                }
            }
            result
        };
        host.runtime_notice(RuntimeNotice::CommandCompleted {
            command: AppCommand::Game(command),
            result: result
                .map(AppOutcome::Command)
                .map_err(|error| error.to_string()),
            success: queued.success,
        });
        report_settlement_issues(host);
    }
    persist_pending(host)
}
/// Settlement runs inside ticks and recalls; its failures become one notice.
fn report_settlement_issues<H: RuntimeHost>(host: &mut H) {
    let issues = host.runtime_parts().0.take_settlement_issues();
    if !issues.is_empty() {
        host.runtime_notice(RuntimeNotice::SettlementIssues(issues));
    }
}
/// The reset is a serialized storage barrier. Live state swaps only on its ack.
fn begin_new_journey<H: RuntimeHost>(host: &mut H, success: String) -> Result<(), String> {
    let (path, _) = host.runtime_paths();
    let old = host.runtime_parts().0.clone();
    let mut fresh = Game::default();
    fresh
        .campaign
        .set_balance_settings(old.campaign.balance_settings.clone())?;
    let cancelled = {
        let (_, runtime) = host.runtime_parts();
        let mut cancelled = runtime.storage.reset(old, fresh, path)?;
        persistence::dedup(&mut cancelled, std::mem::take(&mut runtime.save_requests));
        runtime.reset_success = Some(success);
        runtime.seen.clear();
        runtime.seen.push(AppCommand::NewJourney);
        runtime.clear_input();
        cancelled
    };
    for intent in cancelled {
        host.runtime_notice(RuntimeNotice::Saved {
            intent,
            result: Err("Save superseded by the confirmed new journey".into()),
        });
    }
    Ok(())
}

/// One native FixedUpdate opportunity. Observation speed advances 1/2/4 whole
/// authoritative ticks; one-shot controls are consumed only by the first tick.
pub fn step<H: RuntimeHost>(host: &mut H) {
    let observed = host.runtime_observed_battle();
    let finished = {
        let (game, runtime) = host.runtime_parts();
        runtime.observed_battle = observed;
        runtime.advance(game)
    };
    report_settlement_issues(host);
    if finished {
        host.runtime_notice(RuntimeNotice::SimulationFinished);
    }
}
pub fn apply_commands<H: RuntimeHost>(mut host: ResMut<H>, mut exit: MessageWriter<AppExit>) {
    if flush(&mut *host) {
        exit.write(AppExit::Success);
    }
}
pub fn advance<H: RuntimeHost>(mut host: ResMut<H>) {
    step(&mut *host);
}
pub fn persist<H: RuntimeHost>(mut host: ResMut<H>, mut exit: MessageWriter<AppExit>) {
    if persist_pending(&mut *host) {
        exit.write(AppExit::Success);
    }
}
fn requeue_deferred_saves<H: RuntimeHost>(host: &mut H) {
    let (deferred, rejected) = {
        let (_, runtime) = host.runtime_parts();
        let mut deferred = std::mem::take(&mut runtime.save_requests);
        let mut rejected = Vec::new();
        for queued in std::mem::take(&mut runtime.queue) {
            if let AppCommand::Save(intent) = queued.command {
                persistence::dedup(&mut deferred, [intent]);
            } else {
                rejected.push(queued);
            }
        }
        runtime.seen.retain(|command| {
            !matches!(
                command,
                AppCommand::Save(_) | AppCommand::Game(Command::SetPaused(true))
            )
        });
        (deferred, rejected)
    };
    for queued in rejected {
        host.runtime_notice(RuntimeNotice::CommandCompleted {
            command: queued.command,
            result: Err("Command superseded by the new journey transition".into()),
            success: queued.success,
        });
    }
    if deferred.iter().any(|i| {
        matches!(
            i,
            SaveIntent::Menu | SaveIntent::Quit | SaveIntent::SettingsBack
        )
    }) {
        host.runtime_parts().1.enqueue(Command::SetPaused(true), "");
    }
    for intent in deferred {
        host.runtime_parts().1.enqueue(AppCommand::Save(intent), "");
    }
}

fn poll_storage<H: RuntimeHost>(host: &mut H) -> bool {
    let mut quit = false;
    loop {
        let done = host.runtime_parts().1.storage.poll();
        let Some(done) = done else {
            break;
        };
        if !done.recovered.is_empty() {
            host.runtime_notice(RuntimeNotice::RecoveryCopies(done.recovered));
        }
        if done.epoch != host.runtime_parts().1.storage.epoch() {
            continue;
        }
        if done.reset {
            let success = host
                .runtime_parts()
                .1
                .reset_success
                .take()
                .unwrap_or_default();
            let result = match done.result {
                Ok(persistence::Commit::Reset { game, archive }) => {
                    let (current, runtime) = host.runtime_parts();
                    *current = *game;
                    runtime.clear_input();
                    runtime.controls = ControlFrame::default();
                    runtime.sounds.clear();
                    runtime.observed_battle = None;
                    runtime.dirty = false;
                    runtime.save_ticks = 0;
                    runtime.retry_save_at = 0;
                    Ok(AppOutcome::JourneyArchived(archive))
                }
                Ok(persistence::Commit::Saved) => {
                    Err("Storage reset acknowledgement did not match its command".into())
                }
                Err(error) => Err(error),
            };
            requeue_deferred_saves(host);
            host.runtime_notice(RuntimeNotice::CommandCompleted {
                command: AppCommand::NewJourney,
                result,
                success,
            });
        } else {
            let result = done.result.and_then(|commit| match commit {
                persistence::Commit::Saved => Ok(()),
                _ => Err("Storage acknowledgement mismatch".into()),
            });
            let runtime = host.runtime_parts().1;
            if result.is_err() {
                runtime.dirty = true;
                runtime.retry_save_at = runtime.simulated_ticks + 600;
            } else {
                runtime.retry_save_at = 0;
            }
            quit |= result.is_ok() && done.intents.contains(&SaveIntent::Quit);
            for intent in done.intents {
                host.runtime_notice(RuntimeNotice::Saved {
                    intent,
                    result: result.clone(),
                });
            }
        }
    }
    quit
}
fn persist_pending<H: RuntimeHost>(host: &mut H) -> bool {
    // This main-thread phase measures only polling, snapshot capture and enqueue.
    let _work = crate::diagnostics::work_timer(crate::diagnostics::WorkPhase::Persistence);
    let quit = poll_storage(host);
    let intents = {
        let (_, runtime) = host.runtime_parts();
        if runtime.storage.reset_pending() {
            return quit;
        }
        if runtime.dirty && runtime.simulated_ticks >= runtime.retry_save_at {
            runtime.request_save(SaveIntent::Autosave);
        }
        std::mem::take(&mut runtime.save_requests)
    };
    if intents.is_empty() {
        return quit;
    }
    let (campaign_path, settings_path) = host.runtime_paths();
    let settings = host.runtime_settings().clone();
    let result = {
        let (game, runtime) = host.runtime_parts();
        let snapshot = persistence::Snapshot {
            game: game.clone(),
            settings,
            campaign_path,
            settings_path,
        };
        let result = runtime.storage.save(snapshot, intents.clone());
        if result.is_ok() {
            runtime.dirty = false;
            runtime.save_ticks = 0;
        } else {
            runtime.dirty = true;
            runtime.retry_save_at = runtime.simulated_ticks + 600;
        }
        result
    };
    if let Err(error) = result {
        for intent in intents {
            host.runtime_notice(RuntimeNotice::Saved {
                intent,
                result: Err(error.clone()),
            });
        }
    }
    quit
}
#[cfg(test)]
pub(crate) fn poll_one_save_for_test<H: RuntimeHost>(host: &mut H) -> bool {
    persistence::wait_for_test(&mut host.runtime_parts().1.storage);
    persist_pending(host)
}
/// Test drivers may wait for real worker acknowledgements without advancing any
/// game clock. Production enqueue/poll never waits for filesystem completion.
#[cfg(test)]
pub(crate) fn flush_for_test<H: RuntimeHost>(host: &mut H) -> bool {
    let mut quit = flush(host);
    loop {
        if !host.runtime_parts().1.storage.pending() {
            break;
        }
        persistence::wait_for_test(&mut host.runtime_parts().1.storage);
        quit |= persist_pending(host);
    }
    quit
}
fn preserve_unreadable(
    path: &std::path::Path,
    preserved: &mut Vec<(PathBuf, u64)>,
) -> Result<Vec<PathBuf>, String> {
    use std::{
        hash::{Hash, Hasher},
        io::Write,
        sync::atomic::{AtomicU64, Ordering},
    };
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let mut copies = Vec::new();
    for source in [path.to_path_buf(), path.with_extension("bak")] {
        if !source
            .try_exists()
            .map_err(|error| format!("Cannot inspect {}: {error}", source.display()))?
        {
            continue;
        }
        let bytes = std::fs::read(&source)
            .map_err(|error| format!("Cannot preserve {}: {error}", source.display()))?;
        if Game::decode_save_generation(&bytes).is_ok() {
            continue;
        }
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        bytes.hash(&mut hash);
        let fingerprint = hash.finish();
        if preserved
            .iter()
            .any(|(saved, value)| saved == &source && *value == fingerprint)
        {
            continue;
        }
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|error| error.to_string())?
            .as_nanos();
        let name = source
            .file_name()
            .ok_or("Save path has no filename")?
            .to_string_lossy();
        let copy = source.with_file_name(format!(
            "{name}-recovery-{stamp}-{}.json",
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&copy)
            .map_err(|error| {
                format!(
                    "Cannot preserve unreadable {} before replacement: {error}",
                    source.display()
                )
            })?;
        file.write_all(&bytes)
            .and_then(|()| file.sync_all())
            .map_err(|error| {
                format!(
                    "Cannot synchronize recovery copy {}: {error}",
                    copy.display()
                )
            })?;
        preserved.push((source, fingerprint));
        copies.push(copy);
    }
    Ok(copies)
}
fn write_settings(settings: &Settings, path: &std::path::Path) -> Result<(), String> {
    let temporary = path.with_extension("tmp");
    settings.save(&temporary)?;
    std::fs::OpenOptions::new()
        .write(true)
        .open(&temporary)
        .and_then(|file| file.sync_all())
        .map_err(|error| error.to_string())?;
    std::fs::rename(&temporary, path).map_err(|error| error.to_string())
}

pub const MAX_EFFECT_VOICES: usize = crate::audio::MAX_EFFECT_VOICES;
#[derive(Component)]
pub struct EffectVoice {
    priority: u8,
    last_gain: f32,
    cue_gain: f32,
    cue: SoundCue,
}
fn safe_bus_gain(master: f32, effects: f32) -> f32 {
    fn level(x: f32) -> f32 {
        if x.is_finite() {
            x.clamp(0.0, 1.0)
        } else {
            0.0
        }
    }
    level(master) * level(effects) * 0.7
}
fn select_sounds(mut sounds: Vec<SoundCue>, rotation: u64) -> Vec<SoundCue> {
    // A warning already communicates arrival; avoid stacking a second spawn cue.
    if sounds.contains(&SoundCue::Wave) {
        sounds.retain(|cue| *cue != SoundCue::Incoming);
    }
    sounds.sort_by_key(|cue| cue.priority());
    sounds.dedup();
    let mut selected = Vec::with_capacity(2);
    selected.extend(
        sounds
            .iter()
            .copied()
            .filter(|cue| cue.priority() <= 3)
            .take(2),
    );
    if selected.is_empty()
        && let Some(cue) = sounds
            .iter()
            .copied()
            .find(|cue| !cue.fire() && cue.priority() <= 8)
    {
        selected.push(cue);
    }
    let fire: Vec<_> = sounds.iter().copied().filter(|cue| cue.fire()).collect();
    for n in 0..fire.len() {
        if selected.len() == 2 {
            break;
        }
        selected.push(fire[(rotation as usize + n) % fire.len()]);
    }
    for cue in sounds {
        if selected.len() == 2 {
            break;
        }
        if !selected.contains(&cue) {
            selected.push(cue);
        }
    }
    selected
}
pub fn play_sounds<H: RuntimeHost>(
    mut commands: Commands,
    assets: Res<AssetServer>,
    time: Res<Time<Real>>,
    mut host: ResMut<H>,
    voices: Query<(Entity, &EffectVoice)>,
) {
    let gain = safe_bus_gain(
        host.runtime_settings().master,
        host.runtime_settings().effects,
    );
    let reduced = host.runtime_settings().reduced_effects;
    let now_ms = (time.elapsed_secs_f64() * 1000.0) as u64;
    let (sounds, rotation) = {
        let (_, runtime) = host.runtime_parts();
        let sounds = std::mem::take(&mut runtime.sounds);
        let rotation = runtime.audio_rotation;
        runtime.audio_rotation = runtime
            .audio_rotation
            .wrapping_add(u64::from(!sounds.is_empty()));
        (sounds, rotation)
    };
    if gain == 0.0 {
        return;
    }
    let mut live: Vec<_> = voices
        .iter()
        .map(|(entity, voice)| (entity, voice.priority))
        .collect();
    for cue in select_sounds(sounds, rotation) {
        let replace = if live.len() >= MAX_EFFECT_VOICES {
            let Some((index, (_, priority))) = live
                .iter()
                .enumerate()
                .max_by_key(|(_, (_, priority))| *priority)
            else {
                continue;
            };
            if cue.priority() >= *priority {
                continue;
            }
            Some(index)
        } else {
            None
        };
        if !host.runtime_parts().1.cue_gate.accept(cue, now_ms, reduced) {
            continue;
        }
        if let Some(index) = replace {
            let (entity, _) = live.remove(index);
            commands.entity(entity).despawn();
        }
        let cue_gain = cue.gain(reduced);
        let voice_gain = gain * cue_gain;
        let entity = commands
            .spawn((
                EffectVoice {
                    priority: cue.priority(),
                    last_gain: voice_gain,
                    cue_gain,
                    cue,
                },
                AudioPlayer::new(assets.load(crate::audio::variant_path(cue, rotation))),
                PlaybackSettings::DESPAWN.with_volume(bevy::audio::Volume::Linear(voice_gain)),
            ))
            .id();
        live.push((entity, cue.priority()));
    }
}
/// Both pending PlaybackSettings and live sinks follow the current SFX bus.
pub fn update_effects_gain<H: RuntimeHost>(
    host: Res<H>,
    mut voices: Query<(
        &mut EffectVoice,
        &mut PlaybackSettings,
        Option<&mut AudioSink>,
    )>,
) {
    let gain = safe_bus_gain(
        host.runtime_settings().master,
        host.runtime_settings().effects,
    );
    let reduced = host.runtime_settings().reduced_effects;
    for (mut voice, mut settings, sink) in &mut voices {
        voice.cue_gain = voice.cue.gain(reduced);
        let voice_gain = gain * voice.cue_gain;
        if (voice.last_gain - voice_gain).abs() > 0.00001 {
            settings.volume = bevy::audio::Volume::Linear(voice_gain);
            if let Some(mut sink) = sink {
                sink.set_volume(settings.volume);
            }
            voice.last_gain = voice_gain;
        }
    }
}

pub struct LoadedProfile {
    pub game: Game,
    pub settings: Settings,
    pub warning: String,
}
/// Keep resumable practice profiles separate; screenshot fixtures are ephemeral.
pub fn profile_directory(base: PathBuf, scenario: Option<&str>, practice: bool) -> PathBuf {
    if practice {
        let tag: String = scenario
            .unwrap_or("battle")
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
            .take(48)
            .collect();
        base.join("practice")
            .join(if tag.is_empty() { "battle" } else { &tag })
    } else if scenario.is_some() {
        std::env::temp_dir().join(format!("lastlight-review-{}", std::process::id()))
    } else {
        base
    }
}
/// Startup read service, separate from widget layout and the command runner.
pub fn load_profile(directory: PathBuf) -> LoadedProfile {
    let campaign_path = directory.join("campaign.json");
    let settings_path = directory.join("settings.json");
    let (game, warning) = if campaign_path.exists() || campaign_path.with_extension("bak").exists()
    {
        match Game::load(&campaign_path) {
            Ok(game) => (
                game,
                "Campaign restored. Continue to resume the exact deployment.".into(),
            ),
            Err(error) => (
                Game::default(),
                format!(
                    "Save could not be read: {error}. Starting fresh in memory. Original files remain unchanged; unreadable generations will be preserved as recovery copies before any new save replaces them."
                ),
            ),
        }
    } else {
        (Game::default(), String::new())
    };
    LoadedProfile {
        game,
        settings: settings::Settings::load(&settings_path),
        warning,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    #[test]
    fn country_command_requires_confirmed_survivor_and_retains_cosmetic_identity() {
        use horde_core::survivors::{Appearance, SurvivorId};
        let mut game = Game::default();
        let before = serde_json::to_value(&game).unwrap();
        assert!(game.apply(&Command::ChooseStart("DEU".into())).is_err());
        assert_eq!(serde_json::to_value(&game).unwrap(), before);
        game.apply(&Command::ChooseSurvivor(SurvivorId::June))
            .unwrap();
        game.apply(&Command::ChooseStart("DEU".into())).unwrap();
        assert_eq!(
            game.attempt.as_ref().unwrap().profile.appearance,
            Appearance::Female
        );
    }
    #[test]
    fn authoritative_events_select_weapon_and_ordnance_sounds_without_mutating_the_game() {
        use horde_core::sim::{Actor, SimEvent};
        let mut host = Host::new();
        host.deploy();
        let before = serde_json::to_value(&host.game).unwrap();
        let a = host.game.attempt.as_ref().unwrap();
        let shot = SimEvent::Shot {
            shooter: Actor::Defender {
                mode: a.mode,
                weapon: a.profile.weapon.class,
            },
            target_id: None,
            lane: None,
            from: horde_core::sim::DEFENDER,
            to: [15.0, 5.5],
        };
        host.runtime.event_sound(&shot);
        assert_eq!(host.runtime.sounds, vec![SoundCue::Pistol]);
        host.runtime.sounds.clear();
        let throw = SimEvent::Throw {
            projectile_id: 7,
            ordnance: OrdnanceKind::Grenade,
            lane: Some(5),
            from: horde_core::sim::DEFENDER,
            to: [15.0, 5.5],
            flight_ticks: 30,
        };
        host.runtime.event_sound(&throw);
        assert_eq!(host.runtime.sounds, vec![SoundCue::Throw]);
        host.runtime.sounds.clear();
        for (ordnance, cue) in [
            (OrdnanceKind::Grenade, SoundCue::Explosion),
            (OrdnanceKind::Molotov, SoundCue::Molotov),
        ] {
            host.runtime.event_sound(&SimEvent::Explosion {
                projectile_id: 7,
                ordnance,
                lane: Some(5),
                at: [15.0, 5.5],
                radius: 1.5,
            });
            assert_eq!(host.runtime.sounds, vec![cue]);
            host.runtime.sounds.clear();
        }
        assert_eq!(serde_json::to_value(&host.game).unwrap(), before);
    }
    #[test]
    fn a_sound_emitted_on_the_finishing_tick_is_played() {
        let mut host = Host::new();
        host.deploy();
        let a = host.game.attempt.as_mut().unwrap();
        // The reload completes on the same tick the overrun defender retreats.
        a.hp = 0;
        a.reload_left = 1;
        host.runtime.publish(ControlFrame {
            enabled: true,
            ..ControlFrame::default()
        });
        step(&mut host);
        assert!(host.game.attempt.is_none());
        assert!(host.runtime.sounds.contains(&SoundCue::Reload));
        assert!(
            host.notices
                .iter()
                .any(|n| matches!(n, RuntimeNotice::SimulationFinished))
        );
    }
    #[test]
    fn melee_readiness_event_plays_the_ready_cue() {
        let mut host = Host::new();
        host.deploy();
        host.game.attempt.as_mut().unwrap().melee_cooldown = 1;
        host.runtime.publish(ControlFrame {
            enabled: true,
            ..ControlFrame::default()
        });
        step(&mut host);
        assert!(host.runtime.sounds.contains(&SoundCue::Ready));
    }
    #[test]
    fn authored_node_purchase_command_charges_once_and_preserves_id_ownership() {
        let mut game = Game::default();
        game.campaign.coins = 100;
        game.apply(&Command::BuySkillNode("steady_hands".into()))
            .unwrap();
        assert!(game.campaign.has_node("steady_hands"));
        assert_eq!(game.campaign.coins, 80);
        let before = game.clone();
        assert!(
            game.apply(&Command::BuySkillNode("steady_hands".into()))
                .is_err()
        );
        assert_eq!(
            serde_json::to_value(game).unwrap(),
            serde_json::to_value(before).unwrap()
        );
    }
    #[test]
    fn runtime_consumes_throw_edge_once_even_at_observation_speed_four() {
        use horde_core::ordnance::{OrdnanceKind, ThrowRequest};
        let request = ThrowRequest {
            kind: OrdnanceKind::Grenade,
            cell: [20, 5],
        };
        let mut runtime = Runtime::default();
        runtime.publish(ControlFrame {
            enabled: true,
            speed: 4,
            pressed: Input {
                throw: Some(request),
                ..Input::default()
            },
            held: Input {
                throw: Some(request),
                ..Input::default()
            },
            ..ControlFrame::default()
        });
        assert_eq!(runtime.input().throw, Some(request));
        assert!(runtime.input().throw.is_none());
        assert!(runtime.input().throw.is_none());
    }
    #[test]
    fn trap_shots_never_fabricate_a_turret_and_missing_sources_stay_silent() {
        use horde_core::sim::{Actor, SimEvent, Strike};
        let mut runtime = Runtime::default();
        let shot = |shooter| SimEvent::Shot {
            shooter,
            target_id: None,
            lane: Some(0),
            from: [4.2, 0.5],
            to: [5.0, 0.5],
        };
        let trap = Actor::Structure {
            id: 9000,
            kind: StructureKind::Trap,
        };
        runtime.event_sound(&shot(trap));
        assert_eq!(runtime.sounds, vec![SoundCue::Trap]);
        runtime.sounds.clear();
        runtime.event_sound(&shot(Actor::Structure {
            id: 9000,
            kind: StructureKind::Turret,
        }));
        assert_eq!(runtime.sounds, vec![SoundCue::TurretShot]);
        runtime.sounds.clear();
        runtime.event_sound(&shot(Actor::Unknown { id: 9000 }));
        assert!(runtime.sounds.is_empty());
        runtime.event_sound(&SimEvent::Miss(Strike {
            attacker: trap,
            target_id: Some(1),
            lane: Some(0),
            from: [4.2, 0.5],
            to: [5.0, 0.5],
            value: 0,
        }));
        assert_eq!(runtime.sounds, vec![SoundCue::Trap]);
    }

    #[test]
    fn important_audio_cues_are_not_displaced_by_fire_and_sources_take_turns() {
        let selected = select_sounds(
            vec![
                SoundCue::Rifle,
                SoundCue::TurretShot,
                SoundCue::Wave,
                SoundCue::Incoming,
            ],
            0,
        );
        assert!(selected.contains(&SoundCue::Wave));
        assert_eq!(selected.len(), 2);
        assert_eq!(
            select_sounds(vec![SoundCue::Boss, SoundCue::Damage, SoundCue::Rifle], 0),
            vec![SoundCue::Boss, SoundCue::Damage]
        );
        assert_eq!(
            select_sounds(vec![SoundCue::Wave, SoundCue::Incoming], 0),
            vec![SoundCue::Wave]
        );
        let a = select_sounds(
            vec![SoundCue::Armor, SoundCue::NpcShot, SoundCue::TurretShot],
            0,
        );
        let b = select_sounds(
            vec![SoundCue::Armor, SoundCue::NpcShot, SoundCue::TurretShot],
            1,
        );
        assert!(a.contains(&SoundCue::Armor) && a.contains(&SoundCue::NpcShot));
        assert!(b.contains(&SoundCue::Armor) && b.contains(&SoundCue::TurretShot));
    }
    #[test]
    fn effect_bus_updates_pending_voices_and_mutes_without_recreating_them() {
        let mut host = Host::new();
        host.settings.master = 0.0;
        let mut app = App::new();
        app.insert_resource(host)
            .add_systems(Update, update_effects_gain::<Host>);
        let voice = app
            .world_mut()
            .spawn((
                EffectVoice {
                    priority: 9,
                    last_gain: 0.7,
                    cue_gain: 1.0,
                    cue: SoundCue::Explosion,
                },
                PlaybackSettings::DESPAWN.with_volume(bevy::audio::Volume::Linear(0.7)),
            ))
            .id();
        app.update();
        assert_eq!(
            app.world()
                .entity(voice)
                .get::<PlaybackSettings>()
                .unwrap()
                .volume,
            bevy::audio::Volume::Linear(0.0)
        );
        {
            let mut host = app.world_mut().resource_mut::<Host>();
            host.settings.master = 1.0;
            host.settings.effects = 0.5;
        }
        app.update();
        assert_eq!(
            app.world()
                .entity(voice)
                .get::<PlaybackSettings>()
                .unwrap()
                .volume,
            bevy::audio::Volume::Linear(0.35)
        );
        app.world_mut()
            .resource_mut::<Host>()
            .settings
            .reduced_effects = true;
        app.update();
        let reduced = app
            .world()
            .entity(voice)
            .get::<PlaybackSettings>()
            .unwrap()
            .volume;
        assert_eq!(
            reduced,
            bevy::audio::Volume::Linear(0.35 * SoundCue::Explosion.gain(true))
        );
        assert_eq!(safe_bus_gain(f32::NAN, 1.0), 0.0);
        assert_eq!(safe_bus_gain(9.0, 9.0), 0.7);
    }

    #[derive(Resource)]
    struct Host {
        game: Game,
        runtime: Runtime,
        settings: Settings,
        directory: PathBuf,
        notices: Vec<RuntimeNotice>,
    }
    impl Host {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let directory = std::env::temp_dir().join(format!(
                "lastlight-runtime-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            let mut game = Game::default();
            game.campaign.legacy_features = true;
            // Runtime fixtures below explicitly preserve the legacy11×30 geometry.
            game.campaign
                .set_balance_settings(horde_core::balance::BalanceSettings::legacy())
                .unwrap();
            game.campaign
                .choose_survivor(horde_core::survivors::SurvivorId::Rook)
                .unwrap();
            Self {
                game,
                runtime: Runtime::default(),
                settings: Settings::default(),
                directory,
                notices: vec![],
            }
        }
        fn deploy(&mut self) {
            self.game.campaign.choose_start_country("DEU").unwrap();
            self.game.deploy_country_stage(0, Mode::Personal).unwrap();
            self.game.set_paused(false);
        }
    }
    impl Drop for Host {
        fn drop(&mut self) {
            self.runtime.finish_storage_for_test();
            let archives = self.directory.join("journey-archives");
            if let (Ok(base), Ok(target)) = (self.directory.canonicalize(), archives.canonicalize())
                && target.starts_with(&base)
                && target != base
            {
                let _ = std::fs::remove_dir_all(target);
            }

            for notice in &self.notices {
                if let RuntimeNotice::RecoveryCopies(paths) = notice {
                    for path in paths {
                        if path.parent() == Some(self.directory.as_path()) {
                            let _ = std::fs::remove_file(path);
                        }
                    }
                }
            }
            for file in [
                "campaign.json",
                "campaign.bak",
                "campaign.tmp",
                "settings.json",
                "settings.tmp",
                "not_directory",
            ] {
                let _ = std::fs::remove_file(self.directory.join(file));
            }
            let _ = std::fs::remove_dir(&self.directory);
        }
    }
    impl RuntimeHost for Host {
        fn runtime_parts(&mut self) -> (&mut Game, &mut Runtime) {
            (&mut self.game, &mut self.runtime)
        }
        fn runtime_settings(&self) -> &Settings {
            &self.settings
        }
        fn runtime_paths(&self) -> (PathBuf, PathBuf) {
            (
                self.directory.join("campaign.json"),
                self.directory.join("settings.json"),
            )
        }
        fn runtime_notice(&mut self, notice: RuntimeNotice) {
            self.notices.push(notice);
        }
    }
    fn state(game: &Game) -> serde_json::Value {
        serde_json::to_value(game).unwrap()
    }

    #[test]
    fn repeated_ui_layout_pass_cannot_repeat_a_purchase() {
        let mut host = Host::new();
        host.game.campaign.coins = 100;
        host.runtime.begin_frame(20);
        assert!(
            host.runtime
                .enqueue(Command::BuySkillNode("quick_reload".into()), "Learned")
        );
        host.runtime.begin_frame(20);
        assert!(
            !host
                .runtime
                .enqueue(Command::BuySkillNode("quick_reload".into()), "Learned")
        );
        assert!(!flush_for_test(&mut host));
        assert_eq!(host.game.campaign.coins, 75);
        assert!(host.game.campaign.has_node("quick_reload"));
        assert_eq!(
            host.notices
                .iter()
                .filter(|n| matches!(n, RuntimeNotice::CommandCompleted { .. }))
                .count(),
            1
        );
        assert!(host.directory.join("campaign.json").exists());
    }
    #[test]
    fn rejected_commands_and_settlement_failures_reach_notices_as_text() {
        let mut host = Host::new();
        let tactics = horde_core::command::NpcTactics {
            policy: host.game.campaign.npc.policy,
            prefer_aim: false,
            early_reload: false,
            allow_risk: false,
            recall_percent: 76,
        };
        host.runtime.enqueue(Command::SetNpcPolicy(tactics), "");
        let countries: Vec<_> = horde_core::world::world()
            .countries
            .iter()
            .take(4)
            .map(|c| (c.id.clone(), c.level_count()))
            .collect();
        host.game
            .campaign
            .choose_start_country(&countries[0].0)
            .unwrap();
        for (id, count) in countries {
            host.game
                .campaign
                .world
                .country_levels
                .insert(id.clone(), count);
            host.game.campaign.world.cleared.push(id);
        }
        host.game.campaign.coins = 10_000;
        host.game.campaign.attempts = 1;
        let country = host.game.campaign.world.cleared[0].clone();
        let recruit = host
            .game
            .strategy
            .recruit(&mut host.game.campaign, &country)
            .unwrap();
        let battle = host.game.deploy_survivor(recruit, &country, 0).unwrap();
        host.game.campaign.next_item_id = u64::MAX;
        host.game.battles[0]
            .game
            .campaign
            .add_item_with_quality(4, 9, None)
            .unwrap();
        host.runtime.enqueue(Command::RecallSurvivor(battle), "");
        flush(&mut host);
        assert!(host.notices.iter().any(|n| matches!(
            n,
            RuntimeNotice::CommandCompleted { result: Err(error), .. }
                if error == "Recall threshold must be between 0 and 75 percent"
        )));
        assert!(host.notices.iter().any(|n| matches!(
            n,
            RuntimeNotice::CommandCompleted {
                command: AppCommand::Game(Command::RecallSurvivor(_)),
                result: Ok(AppOutcome::Command(CommandOutcome::Applied)),
                ..
            }
        )));
        let issues: Vec<_> = host
            .notices
            .iter()
            .filter_map(|n| match n {
                RuntimeNotice::SettlementIssues(issues) => Some(issues.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(issues.len(), 1);
        assert!(matches!(
            issues[0].as_slice(),
            [SettlementIssue::LootLost { battle: b, .. }] if *b == battle
        ));
        assert!(host.game.take_settlement_issues().is_empty());
    }
    #[test]
    fn pressed_reload_is_consumed_once_across_fixed_ticks_and_speed() {
        let mut host = Host::new();
        host.deploy();
        host.game.attempt.as_mut().unwrap().ammo = 0;
        host.runtime.publish(ControlFrame {
            enabled: true,
            speed: 4,
            held: Input {
                fire: true,
                auto_reload: false,
                ..Input::default()
            },
            pressed: Input {
                reload: true,
                auto_reload: false,
                ..Input::default()
            },
            pressed_aims: AttackAims::default(),
        });
        for _ in 0..125 {
            step(&mut host);
        }
        let attempt = host.game.attempt.as_ref().unwrap();
        assert_eq!(attempt.tick, 500);
        assert_eq!(attempt.shots, attempt.magazine);
        assert_eq!(attempt.ammo, 0);
        assert_eq!(attempt.reload_left, 0);
    }
    #[test]
    fn pause_freezes_main_parallel_and_strategy_without_catch_up() {
        let mut host = Host::new();
        host.deploy();
        host.game.set_paused(true);
        host.runtime.publish(ControlFrame {
            enabled: true,
            speed: 4,
            held: Input {
                fire: true,
                ..Input::default()
            },
            pressed: Input {
                reload: true,
                ..Input::default()
            },
            pressed_aims: AttackAims::default(),
        });
        let before = state(&host.game);
        for _ in 0..120 {
            step(&mut host);
        }
        assert_eq!(before, state(&host.game));
        assert_eq!(host.runtime.simulated_ticks, 0);
        host.game.set_paused(false);
        step(&mut host);
        assert_eq!(host.game.attempt.as_ref().unwrap().tick, 4);
        assert_eq!(host.game.attempt.as_ref().unwrap().shots, 0);
    }
    #[test]
    fn observation_speed_changes_only_number_of_whole_ticks() {
        let mut normal = Host::new();
        normal.deploy();
        let mut fast = Host::new();
        fast.game = normal.game.clone();
        normal.runtime.publish(ControlFrame {
            enabled: true,
            speed: 1,
            ..ControlFrame::default()
        });
        fast.runtime.publish(ControlFrame {
            enabled: true,
            speed: 4,
            ..ControlFrame::default()
        });
        for _ in 0..240 {
            step(&mut normal);
        }
        for _ in 0..60 {
            step(&mut fast);
        }
        assert_eq!(state(&normal.game), state(&fast.game));
        assert_eq!(normal.runtime.simulated_ticks, fast.runtime.simulated_ticks);
    }
    #[test]
    fn quit_is_acknowledged_only_after_durable_current_state_commit() {
        let mut host = Host::new();
        host.deploy();
        host.game.attempt.as_mut().unwrap().reload_left = 37;
        host.runtime.request_save(SaveIntent::Quit);
        assert!(flush_for_test(&mut host));
        let restored = Game::load(&host.directory.join("campaign.json")).unwrap();
        assert_eq!(state(&host.game), state(&restored));
        assert!(host.notices.iter().any(|n| matches!(
            n,
            RuntimeNotice::Saved {
                intent: SaveIntent::Quit,
                result: Ok(())
            }
        )));
    }
    #[test]
    fn failed_commit_never_acknowledges_quit_or_runs_a_hidden_tick() {
        let mut host = Host::new();
        host.deploy();
        std::fs::create_dir_all(&host.directory).unwrap();
        std::fs::write(
            host.directory.join("campaign.json"),
            b"invalid existing save",
        )
        .unwrap();
        // A directory at the temporary-file path makes the atomic write fail.
        std::fs::create_dir(host.directory.join("campaign.tmp")).unwrap();
        let before = state(&host.game);
        host.runtime.request_save(SaveIntent::Quit);
        assert!(!flush_for_test(&mut host));
        assert_eq!(before, state(&host.game));
        assert!(host.notices.iter().any(|n| matches!(
            n,
            RuntimeNotice::Saved {
                intent: SaveIntent::Quit,
                result: Err(_)
            }
        )));
        std::fs::remove_dir(host.directory.join("campaign.tmp")).unwrap();
    }
    #[test]
    fn terminal_main_attempt_stops_remaining_fixed_catch_up_until_new_input() {
        let mut host = Host::new();
        host.deploy();
        let mut attempt = host.game.attempt.take().unwrap();
        attempt.boss_spawned = true;
        host.game
            .spawn_enemy(&mut attempt, horde_core::content::EnemyKind::Brute, 5, true);
        attempt.enemies.last_mut().unwrap().hp = 0;
        host.game.attempt = Some(attempt);
        host.runtime.publish(ControlFrame {
            enabled: true,
            speed: 4,
            ..ControlFrame::default()
        });
        step(&mut host);
        assert!(host.game.attempt.is_none());
        assert_eq!(host.runtime.simulated_ticks, 1);
        for _ in 0..10 {
            step(&mut host);
        }
        assert_eq!(host.runtime.simulated_ticks, 1);
        assert_eq!(
            host.notices
                .iter()
                .filter(|notice| matches!(notice, RuntimeNotice::SimulationFinished))
                .count(),
            1
        );
    }
    #[test]
    fn corrupt_primary_and_backup_are_uniquely_preserved_before_new_campaign_autosave() {
        let mut host = Host::new();
        std::fs::create_dir_all(&host.directory).unwrap();
        let original = b"unreadable primary bytes";
        let backup = b"unreadable backup bytes";
        std::fs::write(host.directory.join("campaign.json"), original).unwrap();
        std::fs::write(host.directory.join("campaign.bak"), backup).unwrap();
        let loaded = load_profile(host.directory.clone());
        assert!(loaded.warning.contains("Original files remain unchanged"));
        host.game = loaded.game;
        host.runtime.begin_frame(1);
        host.runtime.enqueue(
            Command::ChooseSurvivor(horde_core::survivors::SurvivorId::Rook),
            "Survivor selected",
        );
        host.runtime
            .enqueue(Command::ChooseStart("DEU".into()), "New country selected");
        assert!(!flush_for_test(&mut host));
        let copies: Vec<_> = host
            .notices
            .iter()
            .filter_map(|notice| {
                if let RuntimeNotice::RecoveryCopies(paths) = notice {
                    Some(paths)
                } else {
                    None
                }
            })
            .flatten()
            .collect();
        assert_eq!(copies.len(), 2);
        assert_ne!(copies[0], copies[1]);
        let bytes: Vec<_> = copies
            .iter()
            .map(|path| std::fs::read(path).unwrap())
            .collect();
        assert!(bytes.iter().any(|data| data == original));
        assert!(bytes.iter().any(|data| data == backup));
        let restored = Game::load(&host.directory.join("campaign.json")).unwrap();
        assert_eq!(restored.campaign.world.start.as_deref(), Some("DEU"));
    }
    #[test]
    fn practice_profiles_save_and_resume_without_changing_the_ordinary_campaign() {
        let mut main = Host::new();
        main.game.campaign.coins = 7;
        main.runtime.request_save(SaveIntent::Manual);
        flush_for_test(&mut main);
        let normal_bytes = std::fs::read(main.directory.join("campaign.json")).unwrap();
        let directory = profile_directory(main.directory.clone(), Some("operations"), true);
        {
            let mut practice = Host::new();
            practice.directory = directory.clone();
            practice.game.campaign.choose_start_country("DEU").unwrap();
            practice.game.campaign.coins = 300;
            practice.runtime.request_save(SaveIntent::Manual);
            flush_for_test(&mut practice);
            let loaded = load_profile(directory);
            assert_eq!(loaded.game.campaign.coins, 300);
            assert_eq!(loaded.game.campaign.world.start.as_deref(), Some("DEU"));
            assert_eq!(
                std::fs::read(main.directory.join("campaign.json")).unwrap(),
                normal_bytes
            );
        }
        std::fs::remove_dir(main.directory.join("practice")).unwrap();
    }
    #[test]
    fn named_fixed_schedule_advances_once_per_invocation_without_egui() {
        let mut host = Host::new();
        host.deploy();
        host.runtime.publish(ControlFrame {
            enabled: true,
            ..ControlFrame::default()
        });
        let mut app = App::new();
        app.insert_resource(host)
            .add_systems(FixedUpdate, advance::<Host>);
        app.world_mut().run_schedule(FixedUpdate);
        app.world_mut().run_schedule(FixedUpdate);
        assert_eq!(
            app.world()
                .resource::<Host>()
                .game
                .attempt
                .as_ref()
                .unwrap()
                .tick,
            2
        );
    }
    #[test]
    fn confirmed_new_journey_archives_the_exact_current_game_and_preserves_preferences() {
        let mut host = Host::new();
        host.deploy();
        host.game.set_paused(true);
        host.game.campaign.coins = 123;
        host.settings.scale = 1.17;
        let path = host.directory.join("campaign.json");
        host.game.save(&path).unwrap();
        write_settings(&host.settings, &host.directory.join("settings.json")).unwrap();
        let settings = std::fs::read(host.directory.join("settings.json")).unwrap();
        let before = state(&host.game);
        let balance = host.game.campaign.balance_settings.clone();
        host.runtime.enqueue(AppCommand::NewJourney, "");
        assert!(!flush_for_test(&mut host));
        let archive = host
            .notices
            .iter()
            .find_map(|n| match n {
                RuntimeNotice::CommandCompleted {
                    command: AppCommand::NewJourney,
                    result: Ok(AppOutcome::JourneyArchived(Some(path))),
                    ..
                } => Some(path.clone()),
                _ => None,
            })
            .unwrap();
        assert_eq!(state(&Game::load(&archive).unwrap()), before);
        assert!(host.game.campaign.selected_survivor.is_none());
        assert!(host.game.campaign.world.start.is_none());
        assert!(host.game.battles.is_empty());
        assert!(host.game.attempt.is_none());
        assert_eq!(host.game.campaign.coins, 0);
        assert_eq!(host.game.campaign.attempts, 0);
        assert!(!host.game.campaign.journey_active);
        assert!(!host.game.campaign.legacy_features);
        assert_eq!(host.game.campaign.balance_settings, balance);
        assert_eq!(state(&Game::load(&path).unwrap()), state(&host.game));
        assert_eq!(
            std::fs::read(host.directory.join("settings.json")).unwrap(),
            settings
        );
        assert_eq!(host.settings.scale, 1.17);
        assert!(!host.runtime.controls.enabled);
    }
    #[test]
    fn failed_new_journey_archive_or_commit_preserves_the_current_live_and_saved_generation() {
        for archive_failure in [true, false] {
            let mut host = Host::new();
            host.deploy();
            host.game.set_paused(true);
            let path = host.directory.join("campaign.json");
            host.game.save(&path).unwrap();
            let saved = std::fs::read(&path).unwrap();
            let before = state(&host.game);
            if archive_failure {
                std::fs::write(host.directory.join("journey-archives"), b"blocked").unwrap();
            } else {
                std::fs::create_dir(path.with_extension("tmp")).unwrap();
            }
            host.runtime.enqueue(AppCommand::NewJourney, "");
            assert!(!flush_for_test(&mut host));
            assert_eq!(state(&host.game), before);
            assert_eq!(std::fs::read(&path).unwrap(), saved);
            assert!(host.notices.iter().any(|n| matches!(
                n,
                RuntimeNotice::CommandCompleted {
                    command: AppCommand::NewJourney,
                    result: Err(_),
                    ..
                }
            )));
            if archive_failure {
                std::fs::remove_file(host.directory.join("journey-archives")).unwrap();
            } else {
                std::fs::remove_dir(path.with_extension("tmp")).unwrap();
            }
        }
    }
    #[test]
    fn leaving_journey_is_acknowledged_only_after_durable_save_and_never_loses_earned_state() {
        let mut host = Host::new();
        host.deploy();
        host.game.withdraw();
        host.game.set_paused(true);
        let coins = host.game.campaign.coins;
        let records = host.game.campaign.run_history.len();
        host.runtime.enqueue(Command::LeaveJourney, "");
        flush_for_test(&mut host);
        let saved = Game::load(&host.directory.join("campaign.json")).unwrap();
        assert!(!saved.campaign.journey_active);
        assert_eq!(saved.campaign.coins, coins);
        assert_eq!(saved.campaign.run_history.len(), records);
        assert!(host.notices.iter().any(|n| matches!(
            n,
            RuntimeNotice::Saved {
                intent: SaveIntent::Menu,
                result: Ok(())
            }
        )));
    }
    #[test]
    fn stale_quit_cannot_exit_a_new_journey_and_reset_blocks_old_domain_commands_and_ticks() {
        let mut host = Host::new();
        host.deploy();
        host.game.campaign.coins = 100;
        let (service, gate) = persistence::gated();
        host.runtime.storage = service;
        host.runtime.request_save(SaveIntent::Quit);
        assert!(!flush(&mut host));
        gate.started();
        let original = state(&host.game);
        host.runtime.enqueue(AppCommand::NewJourney, "");
        assert!(!flush(&mut host));
        host.runtime
            .enqueue(Command::BuySkillNode("steady_hands".into()), "");
        assert!(!flush(&mut host));
        host.runtime.publish(ControlFrame {
            enabled: true,
            speed: 4,
            ..Default::default()
        });
        step(&mut host);
        assert_eq!(state(&host.game), original);
        assert_eq!(host.runtime.simulated_ticks, 0);
        gate.release();
        persistence::wait_for_test(&mut host.runtime.storage);
        assert!(!persist_pending(&mut host));
        assert!(gate.started().reset);
        assert_eq!(state(&host.game), original);
        gate.release();
        persistence::wait_for_test(&mut host.runtime.storage);
        assert!(!persist_pending(&mut host));
        assert!(host.game.campaign.world.start.is_none());
        assert_eq!(host.game.campaign.coins, 0);
        assert!(!host.notices.iter().any(|n| matches!(
            n,
            RuntimeNotice::Saved {
                intent: SaveIntent::Quit,
                result: Ok(())
            }
        )));
    }
    #[test]
    fn manual_and_navigation_requests_never_acknowledge_an_older_inflight_snapshot() {
        for intent in [
            SaveIntent::Manual,
            SaveIntent::Menu,
            SaveIntent::Quit,
            SaveIntent::SettingsBack,
        ] {
            let mut host = Host::new();
            let (service, gate) = persistence::gated();
            host.runtime.storage = service;
            host.runtime.request_save(SaveIntent::Autosave);
            assert!(!persist_pending(&mut host));
            gate.started();
            host.game.campaign.coins = 77;
            host.settings.master = 0.41;
            host.runtime.request_save(intent);
            assert!(!persist_pending(&mut host));
            assert!(gate.no_next_job());
            assert!(!host.notices.iter().any(
                |n| matches!(n,RuntimeNotice::Saved{intent:found,result:Ok(())} if *found==intent)
            ));
            gate.release();
            persistence::wait_for_test(&mut host.runtime.storage);
            assert!(!persist_pending(&mut host));
            assert_eq!(gate.started().coins, 77);
            assert!(!host.notices.iter().any(
                |n| matches!(n,RuntimeNotice::Saved{intent:found,result:Ok(())} if *found==intent)
            ));
            gate.release();
            persistence::wait_for_test(&mut host.runtime.storage);
            assert_eq!(persist_pending(&mut host), intent == SaveIntent::Quit);
            let saved = Game::load(&host.directory.join("campaign.json")).unwrap();
            assert_eq!(saved.campaign.coins, 77);
            assert_eq!(
                Settings::load(&host.directory.join("settings.json")).master,
                0.41
            );
            assert!(host.notices.iter().any(
                |n| matches!(n,RuntimeNotice::Saved{intent:found,result:Ok(())} if *found==intent)
            ));
        }
    }
    #[test]
    fn critical_save_barrier_rejects_legal_mutations_and_saves_the_ordered_paused_generation() {
        for intent in [SaveIntent::Menu, SaveIntent::Quit, SaveIntent::SettingsBack] {
            let mut host = Host::new();
            host.game.campaign.choose_start_country("DEU").unwrap();
            host.game.campaign.coins = 77;
            assert!(host.game.campaign.can_purchase_node("steady_hands").is_ok());
            let (service, gate) = persistence::gated();
            host.runtime.storage = service;
            host.runtime.enqueue(Command::SetPaused(true), "");
            host.runtime.enqueue(AppCommand::Save(intent), "");
            assert!(host.runtime.durable_transition_pending());
            assert!(!flush(&mut host));
            gate.started();
            assert!(host.game.global_paused);
            let state_before = state(&host.game);
            assert!(
                !host
                    .runtime
                    .enqueue(Command::BuySkillNode("steady_hands".into()), "")
            );
            assert!(!host.runtime.enqueue(
                Command::Deploy {
                    stage: 0,
                    mode: Mode::Personal
                },
                ""
            ));
            host.runtime.publish(ControlFrame {
                enabled: true,
                speed: 4,
                ..Default::default()
            });
            step(&mut host);
            assert_eq!(state(&host.game), state_before);
            assert_eq!(host.runtime.simulated_ticks, 0);
            assert!(!flush(&mut host));
            assert!(gate.no_next_job());
            gate.release();
            persistence::wait_for_test(&mut host.runtime.storage);
            assert_eq!(persist_pending(&mut host), intent == SaveIntent::Quit);
            assert_eq!(
                state(&Game::load(&host.directory.join("campaign.json")).unwrap()),
                state_before
            );
        }
    }
    #[test]
    fn quit_requested_during_reset_is_recaptured_after_fresh_state_and_pause_are_applied() {
        let mut host = Host::new();
        host.game.campaign.coins = 77;
        let (service, gate) = persistence::gated();
        host.runtime.storage = service;
        host.runtime.enqueue(AppCommand::NewJourney, "");
        assert!(!flush(&mut host));
        assert!(gate.started().reset);
        host.runtime.enqueue(AppCommand::Save(SaveIntent::Quit), "");
        assert!(!flush(&mut host));
        gate.release();
        persistence::wait_for_test(&mut host.runtime.storage);
        assert!(!persist_pending(&mut host));
        assert_eq!(host.game.campaign.coins, 0);
        assert!(host.runtime.durable_transition_pending());
        assert!(!flush(&mut host));
        let next = gate.started();
        assert!(!next.reset);
        assert_eq!(next.coins, 0);
        assert!(host.game.global_paused);
        gate.release();
        persistence::wait_for_test(&mut host.runtime.storage);
        assert!(persist_pending(&mut host));
        let saved = Game::load(&host.directory.join("campaign.json")).unwrap();
        assert!(saved.global_paused);
        assert_eq!(saved.campaign.coins, 0);
    }
    #[test]
    fn same_batch_new_journey_then_native_close_preserves_quit_for_the_fresh_paused_generation() {
        let mut host = Host::new();
        host.game.campaign.coins = 77;
        let (service, gate) = persistence::gated();
        host.runtime.storage = service;
        assert!(host.runtime.enqueue(AppCommand::NewJourney, ""));
        // Native close normally enqueues these together. A queued reset may
        // reject this old-state pause; the deferred fresh pause is mandatory.
        host.runtime.enqueue(Command::SetPaused(true), "");
        assert!(host.runtime.enqueue(AppCommand::Save(SaveIntent::Quit), ""));
        assert!(!flush(&mut host));
        assert!(gate.started().reset);
        gate.release();
        persistence::wait_for_test(&mut host.runtime.storage);
        assert!(!persist_pending(&mut host));
        assert_eq!(host.game.campaign.coins, 0);
        assert!(host.runtime.durable_transition_pending());
        assert!(!flush(&mut host));
        let saved = gate.started();
        assert!(!saved.reset);
        assert_eq!(saved.coins, 0);
        assert!(host.game.global_paused);
        gate.release();
        persistence::wait_for_test(&mut host.runtime.storage);
        assert!(persist_pending(&mut host));
        let saved = Game::load(&host.directory.join("campaign.json")).unwrap();
        assert_eq!(saved.campaign.coins, 0);
        assert!(saved.global_paused);
        assert!(saved.campaign.selected_survivor.is_none());
    }
}
