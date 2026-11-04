//! Native application orchestration. Rendering publishes intents; these services
//! apply commands, advance fixed simulation ticks and persist. Sound cues and
//! runtime notices leave as Bevy messages. No Bevy types cross the horde_core
//! boundary.
use crate::{audio::Cue as SoundCue, profile::ProfilePaths, settings::Settings};
use bevy::{app::AppExit, ecs::system::SystemParam, prelude::*};
use horde_core::{
    command::{Command, CommandOutcome},
    content::{Mode, StructureKind},
    ordnance::OrdnanceKind,
    sim::{Actor, BattleId, Game, Input, SettlementIssue, SimEvent},
};
use std::collections::VecDeque;
use std::path::PathBuf;
mod persistence;
#[cfg(feature = "review")]
pub use persistence::PersistenceMetrics;

/// The authoritative game. Only the runner mutates it (through `Game::apply`
/// and fixed ticks); rendering reads it, so its change ticks mean real changes.
#[derive(Resource)]
pub struct GameSession {
    pub game: Game,
}

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
/// Outcomes the frontend reacts to; the UI's notice system turns them into
/// feedback text and screen transitions.
#[derive(Message, Clone, Debug)]
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
#[derive(Resource, Debug, Default)]
pub struct Runtime {
    frame: Option<u32>,
    seen: Vec<AppCommand>,
    queue: VecDeque<QueuedCommand>,
    controls: ControlFrame,
    pending: Input,
    pending_aims: AttackAims,
    save_requests: Vec<SaveIntent>,
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
    fn advance(&mut self, game: &mut Game, cues: &mut Vec<SoundCue>) -> bool {
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
            tick_sounds(game, self.observed_battle, cues);
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

/// Queues `cue` once per batch.
pub(crate) fn push_cue(cues: &mut Vec<SoundCue>, cue: SoundCue) {
    if !cues.contains(&cue) {
        cues.push(cue);
    }
}
fn event_sound(cues: &mut Vec<SoundCue>, event: &SimEvent) {
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
        _ => None,
    };
    if let Some(cue) = cue {
        push_cue(cues, cue);
    }
}
/// Sounds for one tick's drained events. The main deployment is always
/// audible; camp observation hears the observed expedition in full and only
/// important cues from the others, without multiplying ordinary shots.
fn tick_sounds(game: &mut Game, observed: Option<u64>, cues: &mut Vec<SoundCue>) {
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
            event_sound(cues, &record.event);
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
        push_cue(cues, SoundCue::Fire);
    }
}

/// Everything one runner pass touches. Systems build it from resources and
/// forward `notices`/`cues` as messages; tests build it from plain values.
pub struct RuntimeCtx<'a> {
    pub game: &'a mut Game,
    pub runtime: &'a mut Runtime,
    pub settings: &'a Settings,
    pub paths: &'a ProfilePaths,
    pub notices: &'a mut Vec<RuntimeNotice>,
    pub cues: &'a mut Vec<SoundCue>,
    /// Set when the pass mutated `game`, so `GameSession` change ticks stay honest.
    pub game_changed: bool,
}
impl RuntimeCtx<'_> {
    fn notice(&mut self, notice: RuntimeNotice) {
        self.notices.push(notice);
    }
}

/// Pure application service: applies pending typed commands and durable saves.
/// Returns true only when a requested Quit has committed successfully.
pub fn flush(ctx: &mut RuntimeCtx) -> bool {
    let commands = std::mem::take(&mut ctx.runtime.queue);
    let mut commands = commands.into_iter();
    while let Some(queued) = commands.next() {
        // Save and reset intents are durable runner concerns; only gameplay
        // commands reach `Game::apply`.
        let command = match queued.command {
            AppCommand::Game(command) => command,
            AppCommand::Save(intent) => {
                ctx.runtime.request_save(intent);
                continue;
            }
            AppCommand::NewJourney => {
                if let Err(error) = begin_new_journey(ctx, queued.success.clone()) {
                    ctx.notice(RuntimeNotice::CommandCompleted {
                        command: AppCommand::NewJourney,
                        result: Err(error),
                        success: queued.success,
                    });
                }
                // Later save/close intents belong to the confirmed new epoch.
                // They must survive this barrier and be recaptured after reset.
                for trailing in commands.by_ref() {
                    if let AppCommand::Save(intent) = trailing.command {
                        ctx.runtime.request_save(intent);
                    } else {
                        ctx.notice(RuntimeNotice::CommandCompleted {
                            command: trailing.command,
                            result: Err("Command superseded by the new journey transition".into()),
                            success: trailing.success,
                        });
                    }
                }
                if !ctx.runtime.reset_in_progress() {
                    requeue_deferred_saves(ctx);
                }
                break;
            }
        };
        if ctx.runtime.durable_transition_pending() {
            ctx.notice(RuntimeNotice::CommandCompleted {
                command: AppCommand::Game(command),
                result: Err("Wait for the current durable transition to finish".into()),
                success: queued.success,
            });
            continue;
        }
        let result = ctx.game.apply(&command);
        ctx.game_changed = true;
        if result.is_ok() {
            ctx.runtime.dirty = true;
            if matches!(command, Command::LeaveJourney) {
                ctx.runtime.request_save(SaveIntent::Menu);
            }
            if matches!(command, Command::Withdraw) {
                push_cue(ctx.cues, SoundCue::Retreat);
            }
        }
        ctx.notice(RuntimeNotice::CommandCompleted {
            command: AppCommand::Game(command),
            result: result
                .map(AppOutcome::Command)
                .map_err(|error| error.to_string()),
            success: queued.success,
        });
        report_settlement_issues(ctx);
    }
    persist_pending(ctx)
}
/// Settlement runs inside ticks and recalls; its failures become one notice.
fn report_settlement_issues(ctx: &mut RuntimeCtx) {
    let issues = ctx.game.take_settlement_issues();
    if !issues.is_empty() {
        ctx.game_changed = true;
        ctx.notice(RuntimeNotice::SettlementIssues(issues));
    }
}
/// The reset is a serialized storage barrier. Live state swaps only on its ack.
fn begin_new_journey(ctx: &mut RuntimeCtx, success: String) -> Result<(), String> {
    let path = ctx.paths.campaign();
    let old = ctx.game.clone();
    let mut fresh = Game::default();
    fresh
        .campaign
        .set_balance_settings(old.campaign.balance_settings.clone())?;
    let cancelled = {
        let runtime = &mut *ctx.runtime;
        let mut cancelled = runtime.storage.reset(old, fresh, path)?;
        persistence::dedup(&mut cancelled, std::mem::take(&mut runtime.save_requests));
        runtime.reset_success = Some(success);
        runtime.seen.clear();
        runtime.seen.push(AppCommand::NewJourney);
        runtime.clear_input();
        cancelled
    };
    for intent in cancelled {
        ctx.notice(RuntimeNotice::Saved {
            intent,
            result: Err("Save superseded by the confirmed new journey".into()),
        });
    }
    Ok(())
}

/// One native FixedUpdate opportunity. Observation speed advances 1/2/4 whole
/// authoritative ticks; one-shot controls are consumed only by the first tick.
pub fn step(ctx: &mut RuntimeCtx) {
    let before = ctx.runtime.simulated_ticks;
    let finished = ctx.runtime.advance(ctx.game, ctx.cues);
    ctx.game_changed |= ctx.runtime.simulated_ticks != before;
    report_settlement_issues(ctx);
    if finished {
        ctx.notice(RuntimeNotice::SimulationFinished);
    }
}
fn requeue_deferred_saves(ctx: &mut RuntimeCtx) {
    let (deferred, rejected) = {
        let runtime = &mut *ctx.runtime;
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
        ctx.notice(RuntimeNotice::CommandCompleted {
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
        ctx.runtime.enqueue(Command::SetPaused(true), "");
    }
    for intent in deferred {
        ctx.runtime.enqueue(AppCommand::Save(intent), "");
    }
}

fn poll_storage(ctx: &mut RuntimeCtx) -> bool {
    let mut quit = false;
    while let Some(done) = ctx.runtime.storage.poll() {
        if !done.recovered.is_empty() {
            ctx.notice(RuntimeNotice::RecoveryCopies(done.recovered));
        }
        if done.epoch != ctx.runtime.storage.epoch() {
            continue;
        }
        if done.reset {
            let success = ctx.runtime.reset_success.take().unwrap_or_default();
            let result = match done.result {
                Ok(persistence::Commit::Reset { game, archive }) => {
                    *ctx.game = *game;
                    ctx.game_changed = true;
                    let runtime = &mut *ctx.runtime;
                    runtime.clear_input();
                    runtime.controls = ControlFrame::default();
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
            requeue_deferred_saves(ctx);
            ctx.notice(RuntimeNotice::CommandCompleted {
                command: AppCommand::NewJourney,
                result,
                success,
            });
        } else {
            let result = done.result.and_then(|commit| match commit {
                persistence::Commit::Saved => Ok(()),
                _ => Err("Storage acknowledgement mismatch".into()),
            });
            let runtime = &mut *ctx.runtime;
            if result.is_err() {
                runtime.dirty = true;
                runtime.retry_save_at = runtime.simulated_ticks + 600;
            } else {
                runtime.retry_save_at = 0;
            }
            quit |= result.is_ok() && done.intents.contains(&SaveIntent::Quit);
            for intent in done.intents {
                ctx.notice(RuntimeNotice::Saved {
                    intent,
                    result: result.clone(),
                });
            }
        }
    }
    quit
}
fn persist_pending(ctx: &mut RuntimeCtx) -> bool {
    // This main-thread phase measures only polling, snapshot capture and enqueue.
    let _work = crate::diagnostics::work_timer(crate::diagnostics::WorkPhase::Persistence);
    let quit = poll_storage(ctx);
    let intents = {
        let runtime = &mut *ctx.runtime;
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
    let snapshot = persistence::Snapshot {
        game: ctx.game.clone(),
        settings: ctx.settings.clone(),
        campaign_path: ctx.paths.campaign(),
        settings_path: ctx.paths.settings(),
    };
    let result = {
        let runtime = &mut *ctx.runtime;
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
            ctx.notice(RuntimeNotice::Saved {
                intent,
                result: Err(error.clone()),
            });
        }
    }
    quit
}
#[cfg(test)]
pub(crate) fn poll_one_save_for_test(ctx: &mut RuntimeCtx) -> bool {
    persistence::wait_for_test(&mut ctx.runtime.storage);
    persist_pending(ctx)
}
/// Test drivers may wait for real worker acknowledgements without advancing any
/// game clock. Production enqueue/poll never waits for filesystem completion.
#[cfg(test)]
pub(crate) fn flush_for_test(ctx: &mut RuntimeCtx) -> bool {
    let mut quit = flush(ctx);
    loop {
        if !ctx.runtime.storage.pending() {
            break;
        }
        persistence::wait_for_test(&mut ctx.runtime.storage);
        quit |= persist_pending(ctx);
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
            .add_message::<SoundCue>()
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
    cues: MessageWriter<'w, SoundCue>,
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
            };
            let result = pass(&mut ctx);
            (result, ctx.game_changed)
        };
        if changed {
            self.session.set_changed();
        }
        self.notices.write_batch(notices);
        self.cues.write_batch(cues);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::{load_profile, profile_directory};
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
        event_sound(&mut host.cues, &shot);
        assert_eq!(host.cues, vec![SoundCue::Pistol]);
        host.cues.clear();
        let throw = SimEvent::Throw {
            projectile_id: 7,
            ordnance: OrdnanceKind::Grenade,
            lane: Some(5),
            from: horde_core::sim::DEFENDER,
            to: [15.0, 5.5],
            flight_ticks: 30,
        };
        event_sound(&mut host.cues, &throw);
        assert_eq!(host.cues, vec![SoundCue::Throw]);
        host.cues.clear();
        for (ordnance, cue) in [
            (OrdnanceKind::Grenade, SoundCue::Explosion),
            (OrdnanceKind::Molotov, SoundCue::Molotov),
        ] {
            event_sound(
                &mut host.cues,
                &SimEvent::Explosion {
                    projectile_id: 7,
                    ordnance,
                    lane: Some(5),
                    at: [15.0, 5.5],
                    radius: 1.5,
                },
            );
            assert_eq!(host.cues, vec![cue]);
            host.cues.clear();
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
        step(&mut host.ctx());
        assert!(host.game.attempt.is_none());
        assert!(host.cues.contains(&SoundCue::Reload));
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
        step(&mut host.ctx());
        assert!(host.cues.contains(&SoundCue::Ready));
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
        let mut cues = Vec::new();
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
        event_sound(&mut cues, &shot(trap));
        assert_eq!(cues, vec![SoundCue::Trap]);
        cues.clear();
        event_sound(
            &mut cues,
            &shot(Actor::Structure {
                id: 9000,
                kind: StructureKind::Turret,
            }),
        );
        assert_eq!(cues, vec![SoundCue::TurretShot]);
        cues.clear();
        event_sound(&mut cues, &shot(Actor::Unknown { id: 9000 }));
        assert!(cues.is_empty());
        event_sound(
            &mut cues,
            &SimEvent::Miss(Strike {
                attacker: trap,
                target_id: Some(1),
                lane: Some(0),
                from: [4.2, 0.5],
                to: [5.0, 0.5],
                value: 0,
            }),
        );
        assert_eq!(cues, vec![SoundCue::Trap]);
    }

    /// Owned runner state; `ctx()` lends it to the pure runner functions.
    struct Host {
        game: Game,
        runtime: Runtime,
        settings: Settings,
        directory: PathBuf,
        paths: ProfilePaths,
        notices: Vec<RuntimeNotice>,
        cues: Vec<SoundCue>,
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
                paths: ProfilePaths::in_directory(directory.clone()),
                directory,
                notices: vec![],
                cues: vec![],
            }
        }
        fn ctx(&mut self) -> RuntimeCtx<'_> {
            RuntimeCtx {
                game: &mut self.game,
                runtime: &mut self.runtime,
                settings: &self.settings,
                paths: &self.paths,
                notices: &mut self.notices,
                cues: &mut self.cues,
                game_changed: false,
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
        assert!(!flush_for_test(&mut host.ctx()));
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
        flush(&mut host.ctx());
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
            step(&mut host.ctx());
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
            step(&mut host.ctx());
        }
        assert_eq!(before, state(&host.game));
        assert_eq!(host.runtime.simulated_ticks, 0);
        host.game.set_paused(false);
        step(&mut host.ctx());
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
            step(&mut normal.ctx());
        }
        for _ in 0..60 {
            step(&mut fast.ctx());
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
        assert!(flush_for_test(&mut host.ctx()));
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
        assert!(!flush_for_test(&mut host.ctx()));
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
        step(&mut host.ctx());
        assert!(host.game.attempt.is_none());
        assert_eq!(host.runtime.simulated_ticks, 1);
        for _ in 0..10 {
            step(&mut host.ctx());
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
        assert!(!flush_for_test(&mut host.ctx()));
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
        flush_for_test(&mut main.ctx());
        let normal_bytes = std::fs::read(main.directory.join("campaign.json")).unwrap();
        let directory = profile_directory(main.directory.clone(), Some("operations"), true);
        {
            let mut practice = Host::new();
            practice.directory = directory.clone();
            practice.paths = ProfilePaths::in_directory(directory.clone());
            practice.game.campaign.choose_start_country("DEU").unwrap();
            practice.game.campaign.coins = 300;
            practice.runtime.request_save(SaveIntent::Manual);
            flush_for_test(&mut practice.ctx());
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
        app.insert_resource(GameSession {
            game: std::mem::take(&mut host.game),
        })
        .insert_resource(std::mem::take(&mut host.runtime))
        .insert_resource(host.settings.clone())
        .insert_resource(host.paths.clone())
        .add_plugins(SimRuntimePlugin);
        app.world_mut().run_schedule(FixedUpdate);
        app.world_mut().run_schedule(FixedUpdate);
        assert_eq!(
            app.world()
                .resource::<GameSession>()
                .game
                .attempt
                .as_ref()
                .unwrap()
                .tick,
            2
        );
        app.world_mut()
            .resource_mut::<Runtime>()
            .finish_storage_for_test();
    }
    #[test]
    fn paused_fixed_steps_report_no_game_change() {
        let mut host = Host::new();
        host.deploy();
        host.game.set_paused(true);
        host.runtime.publish(ControlFrame {
            enabled: true,
            ..ControlFrame::default()
        });
        let mut ctx = host.ctx();
        step(&mut ctx);
        assert!(!ctx.game_changed, "paused steps must not dirty GameSession");
        ctx.game.set_paused(false);
        step(&mut ctx);
        assert!(ctx.game_changed);
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
        assert!(!flush_for_test(&mut host.ctx()));
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
            assert!(!flush_for_test(&mut host.ctx()));
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
        flush_for_test(&mut host.ctx());
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
        assert!(!flush(&mut host.ctx()));
        gate.started();
        let original = state(&host.game);
        host.runtime.enqueue(AppCommand::NewJourney, "");
        assert!(!flush(&mut host.ctx()));
        host.runtime
            .enqueue(Command::BuySkillNode("steady_hands".into()), "");
        assert!(!flush(&mut host.ctx()));
        host.runtime.publish(ControlFrame {
            enabled: true,
            speed: 4,
            ..Default::default()
        });
        step(&mut host.ctx());
        assert_eq!(state(&host.game), original);
        assert_eq!(host.runtime.simulated_ticks, 0);
        gate.release();
        persistence::wait_for_test(&mut host.runtime.storage);
        assert!(!persist_pending(&mut host.ctx()));
        assert!(gate.started().reset);
        assert_eq!(state(&host.game), original);
        gate.release();
        persistence::wait_for_test(&mut host.runtime.storage);
        assert!(!persist_pending(&mut host.ctx()));
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
            assert!(!persist_pending(&mut host.ctx()));
            gate.started();
            host.game.campaign.coins = 77;
            host.settings.master = 0.41;
            host.runtime.request_save(intent);
            assert!(!persist_pending(&mut host.ctx()));
            assert!(gate.no_next_job());
            assert!(!host.notices.iter().any(
                |n| matches!(n,RuntimeNotice::Saved{intent:found,result:Ok(())} if *found==intent)
            ));
            gate.release();
            persistence::wait_for_test(&mut host.runtime.storage);
            assert!(!persist_pending(&mut host.ctx()));
            assert_eq!(gate.started().coins, 77);
            assert!(!host.notices.iter().any(
                |n| matches!(n,RuntimeNotice::Saved{intent:found,result:Ok(())} if *found==intent)
            ));
            gate.release();
            persistence::wait_for_test(&mut host.runtime.storage);
            assert_eq!(persist_pending(&mut host.ctx()), intent == SaveIntent::Quit);
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
            assert!(!flush(&mut host.ctx()));
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
            step(&mut host.ctx());
            assert_eq!(state(&host.game), state_before);
            assert_eq!(host.runtime.simulated_ticks, 0);
            assert!(!flush(&mut host.ctx()));
            assert!(gate.no_next_job());
            gate.release();
            persistence::wait_for_test(&mut host.runtime.storage);
            assert_eq!(persist_pending(&mut host.ctx()), intent == SaveIntent::Quit);
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
        assert!(!flush(&mut host.ctx()));
        assert!(gate.started().reset);
        host.runtime.enqueue(AppCommand::Save(SaveIntent::Quit), "");
        assert!(!flush(&mut host.ctx()));
        gate.release();
        persistence::wait_for_test(&mut host.runtime.storage);
        assert!(!persist_pending(&mut host.ctx()));
        assert_eq!(host.game.campaign.coins, 0);
        assert!(host.runtime.durable_transition_pending());
        assert!(!flush(&mut host.ctx()));
        let next = gate.started();
        assert!(!next.reset);
        assert_eq!(next.coins, 0);
        assert!(host.game.global_paused);
        gate.release();
        persistence::wait_for_test(&mut host.runtime.storage);
        assert!(persist_pending(&mut host.ctx()));
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
        assert!(!flush(&mut host.ctx()));
        assert!(gate.started().reset);
        gate.release();
        persistence::wait_for_test(&mut host.runtime.storage);
        assert!(!persist_pending(&mut host.ctx()));
        assert_eq!(host.game.campaign.coins, 0);
        assert!(host.runtime.durable_transition_pending());
        assert!(!flush(&mut host.ctx()));
        let saved = gate.started();
        assert!(!saved.reset);
        assert_eq!(saved.coins, 0);
        assert!(host.game.global_paused);
        gate.release();
        persistence::wait_for_test(&mut host.runtime.storage);
        assert!(persist_pending(&mut host.ctx()));
        let saved = Game::load(&host.directory.join("campaign.json")).unwrap();
        assert_eq!(saved.campaign.coins, 0);
        assert!(saved.global_paused);
        assert!(saved.campaign.selected_survivor.is_none());
    }
}
