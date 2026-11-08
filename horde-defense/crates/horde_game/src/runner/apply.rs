//! Pure application services over `RuntimeCtx`: flushing commands, fixed steps,
//! storage polling and durable-save wiring.
use super::{command::*, persistence, runtime::*};
use crate::{audio::Cue as SoundCue, profile::ProfilePaths, settings::Settings};
use horde_core::{command::Command, sim::Game};
use std::path::PathBuf;

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
    /// Perf launches only; tests and normal play pass `None`.
    pub perf: Option<&'a crate::diagnostics::PerfProbe>,
}
impl RuntimeCtx<'_> {
    pub(super) fn notice(&mut self, notice: RuntimeNotice) {
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
        #[cfg(feature = "dev")]
        ctx.runtime.record_command(&command);
        let result = ctx.game.apply(&command);
        ctx.game_changed = true;
        if result.is_ok() {
            ctx.runtime.dirty = true;
            if matches!(command, Command::LeaveJourney) {
                ctx.runtime.request_save(SaveIntent::Menu);
            }
            if matches!(command, Command::Withdraw) {
                // The terminal `AttemptFinished` plays `Retreat` exactly once.
                ctx.runtime.drain_sounds(ctx.game, ctx.cues);
            }
            if confirms_purchase(&command) {
                push_cue(ctx.cues, SoundCue::Confirm);
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
/// Camp purchases, training and equipment changes that earn a confirmation cue.
fn confirms_purchase(command: &Command) -> bool {
    matches!(
        command,
        Command::BuySkillNode(_)
            | Command::BuyItem(_)
            | Command::Equip { .. }
            | Command::EquipRecruit { .. }
            | Command::BuyNpcPoint
            | Command::TrainNpc(_)
            | Command::BuyRecruitPoint(_)
            | Command::TrainRecruit { .. }
            | Command::Recruit(_)
            | Command::RecruitAs { .. }
    )
}
/// Settlement runs inside ticks and recalls; its failures become one notice.
pub(super) fn report_settlement_issues(ctx: &mut RuntimeCtx) {
    let issues = ctx.game.take_settlement_issues();
    if !issues.is_empty() {
        #[cfg(feature = "dev")]
        {
            ctx.runtime.settlement_issue_total += issues.len();
        }
        ctx.game_changed = true;
        ctx.notice(RuntimeNotice::SettlementIssues(issues));
    }
}
/// The reset is a serialized storage barrier. Live state swaps only on its ack.
pub(super) fn begin_new_journey(ctx: &mut RuntimeCtx, success: String) -> Result<(), String> {
    // `Runtime::forbid_saves`: the reset writes an archive and a fresh
    // campaign.json, so an unsaveable review refuses it before any of that.
    if ctx.runtime.unsaveable {
        return Err("This review never saves, so it cannot start a new journey".into());
    }
    // The live state is swapped on the storage acknowledgement, at a tick a
    // recording cannot place, so the recording ends here.
    #[cfg(feature = "dev")]
    ctx.runtime
        .stop_recording(ctx.game, "a new journey cannot be replayed");
    let path = ctx.paths.campaign();
    let old = ctx.game.clone();
    let mut fresh = Game::default();
    // A Custom balance keeps the named preset its extras came from: apply that
    // preset first, so the Custom settings record it as the previous preset.
    let balance = &old.campaign().balance_settings;
    let mut carried = Vec::with_capacity(2);
    if balance.preset == horde_core::balance::BalancePreset::Custom {
        carried.push(horde_core::balance::BalanceSettings::for_preset(
            old.campaign().balance_extras_preset(),
        ));
    }
    carried.push(balance.clone());
    for settings in carried {
        fresh
            .apply(&horde_core::command::Command::SetBalance(settings))
            .map_err(|e| e.to_string())?;
    }
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
    let finished = ctx.runtime.advance(ctx.game, ctx.cues, ctx.perf);
    ctx.game_changed |= ctx.runtime.simulated_ticks != before;
    report_settlement_issues(ctx);
    if finished {
        ctx.notice(RuntimeNotice::SimulationFinished);
    }
}
pub(super) fn requeue_deferred_saves(ctx: &mut RuntimeCtx) {
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

pub(super) fn poll_storage(ctx: &mut RuntimeCtx) -> bool {
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
                Ok(persistence::Commit::Saved | persistence::Commit::SavedWithoutSettings(_)) => {
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
            let mut settings_error = None;
            let result = done.result.and_then(|commit| match commit {
                persistence::Commit::Saved => Ok(()),
                persistence::Commit::SavedWithoutSettings(error) => {
                    settings_error = Some(error);
                    Ok(())
                }
                persistence::Commit::Reset { .. } => Err("Storage acknowledgement mismatch".into()),
            });
            let runtime = &mut *ctx.runtime;
            if result.is_err() || settings_error.is_some() {
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
            if let Some(error) = settings_error {
                ctx.notice(RuntimeNotice::SettingsNotSaved(error));
            }
        }
    }
    quit
}
pub(super) fn persist_pending(ctx: &mut RuntimeCtx) -> bool {
    // This main-thread phase measures only polling, snapshot capture and enqueue.
    let _work =
        crate::diagnostics::work_timer(ctx.perf, crate::diagnostics::WorkPhase::Persistence);
    let quit = poll_storage(ctx);
    let intents = {
        let runtime = &mut *ctx.runtime;
        if runtime.storage.reset_pending() {
            return quit;
        }
        if runtime.unsaveable {
            // `Runtime::forbid_saves`: nothing reaches storage, no autosave.
            // Every user intent still finishes, so its navigation runs.
            runtime.dirty = false;
            let intents = std::mem::take(&mut runtime.save_requests);
            let quit = quit || intents.contains(&SaveIntent::Quit);
            for intent in intents {
                if intent != SaveIntent::Autosave {
                    ctx.notice(RuntimeNotice::SaveSkipped { intent });
                }
            }
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
        // Saved fields only: the transient event outbox stays with the frontend.
        game: ctx.game.save_snapshot(),
        settings: ctx.settings.clone(),
        campaign_path: ctx.paths.campaign(),
        settings_path: ctx.paths.settings(),
        preserve_settings: ctx.runtime.settings_at_risk,
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

pub(super) fn preserve_unreadable(
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
pub(super) fn write_settings(settings: &Settings, path: &std::path::Path) -> Result<(), String> {
    let temporary = path.with_extension("tmp");
    settings.save(&temporary)?;
    std::fs::OpenOptions::new()
        .write(true)
        .open(&temporary)
        .and_then(|file| file.sync_all())
        .map_err(|error| error.to_string())?;
    std::fs::rename(&temporary, path).map_err(|error| error.to_string())
}
