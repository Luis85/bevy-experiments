//! Runtime notices become feedback text and state transitions here, in one
//! system; `OnEnter(Screen::Battle)` owns the reset a new deployment needs.
use super::{
    BattleUi, CampaignUi, Navigation, Screen, Tab, UiCtx, UiParams,
    battle::AttackSelection,
    damage, loot,
    modal::{AppModal, StorageError},
    toast::{RECOVERY_COPIES, TOAST_LONG_SECS, ToastKind},
};
use crate::{
    audio::Cue as SoundCue,
    runner::{
        AppCommand, AppOutcome, GameSession, Runtime, RuntimeNotice, SAVE_SUPERSEDED, SaveIntent,
    },
};
use bevy::prelude::*;
use horde_core::{
    command::{Command, CommandOutcome},
    content::Mode,
    sim::Game,
    survivors::SurvivorId,
};

/// The toast a finished save of any intent but Autosave shows.
const SAVED: &str = "Saved";
/// The same toast when unreadable save bytes were kept as recovery copies
/// (Info, 6 s; the status line keeps the copies' paths).
const SAVED_WITH_RECOVERY: &str = "Saved · recovery copies kept";

/// The single navigation system: reads every runtime notice once (after the
/// runner's Apply pass, before this frame's state transition).
pub(super) fn handle_notices(
    mut notices: MessageReader<RuntimeNotice>,
    mut params: UiParams,
    mut cues: MessageWriter<SoundCue>,
) {
    let notices: Vec<_> = notices.read().cloned().collect();
    if notices.is_empty() {
        return;
    }
    let mut buffer = Vec::new();
    let mut f = params.ctx(&mut buffer);
    for notice in notices {
        f.apply_notice(notice);
    }
    cues.write_batch(buffer);
}

/// `OnEnter(Screen::Battle)`: a new deployment starts with centred aim,
/// gun selected, construction closed, normal speed and no stale input.
pub(super) fn enter_battle(
    mut nav: ResMut<Navigation>,
    mut battle: ResMut<BattleUi>,
    mut campaign: ResMut<CampaignUi>,
    session: Res<GameSession>,
    mut runtime: ResMut<Runtime>,
) {
    reset_new_battle(
        &mut nav,
        &mut battle,
        &mut campaign,
        &session.game,
        &mut runtime,
    );
}
/// Shared by the `OnEnter` system and tests' transition emulation. Only a
/// deployment flagged by `UiCtx::start_battle` resets; returning to a battle
/// from settings or history keeps its controls.
pub(super) fn reset_new_battle(
    nav: &mut Navigation,
    battle: &mut BattleUi,
    campaign: &mut CampaignUi,
    game: &Game,
    runtime: &mut Runtime,
) {
    if !std::mem::take(&mut nav.battle_reset) {
        return;
    }
    if let Some(a) = game.attempt() {
        battle.aim = [a.width_yards * 0.5, a.defender_position()[1]];
        campaign.mode = a.mode;
    }
    battle.set_construction(false);
    battle.attack_selection = AttackSelection::Gun;
    battle.fire_latched = false;
    battle.speed = 1;
    runtime.clear_input();
}

impl UiCtx<'_> {
    /// The navigation a finished save intent runs: after its save, or after
    /// `RuntimeNotice::SaveSkipped` on an unsaveable review.
    fn finish_save_intent(&mut self, intent: SaveIntent) {
        match intent {
            SaveIntent::Menu => {
                self.go(Screen::Menu);
                self.prefs.cancel_rebind();
                self.battle.fire_latched = false;
                self.runtime.clear_input();
            }
            SaveIntent::SettingsBack => {
                self.go(match self.nav.return_screen {
                    Screen::Settings => Screen::Menu,
                    screen => screen,
                });
                self.prefs.cancel_rebind();
                if self.nav.return_screen == Screen::Camp {
                    self.dispatch(Command::SetPaused(self.nav.return_world_paused), "");
                }
            }
            _ => {}
        }
    }
    pub(super) fn apply_notice(&mut self, notice: RuntimeNotice) {
        if matches!(
            &notice,
            RuntimeNotice::CommandCompleted {
                command: AppCommand::NewJourney,
                ..
            }
        ) {
            self.status.save_pending = self.runtime.user_save_pending();
        }
        match notice {
            RuntimeNotice::CommandCompleted {
                command,
                result,
                success,
            } => match result {
                Ok(outcome) => self.command_succeeded(command, outcome, success),
                Err(error) => {
                    self.status.feedback_revision = self.status.feedback_revision.wrapping_add(1);
                    self.status.message = error;
                    self.status.message_error = true;
                }
            },
            RuntimeNotice::Saved { intent, result } => {
                self.status.save_pending = self.runtime.user_save_pending();
                match result {
                    Ok(()) => {
                        self.status.profile_exists = true;
                        // A save went through: the failure is over.
                        self.status.dismissed_save_error = None;
                        if matches!(
                            self.status.modal,
                            Some(AppModal::Storage(StorageError::Save { .. }))
                        ) {
                            self.status.modal = None;
                            self.status.storage_note.clear();
                        }
                        if intent != SaveIntent::Autosave {
                            // The toast says "Saved"; the status line keeps
                            // only what must stay findable (the copies' paths).
                            self.status.feedback_revision =
                                self.status.feedback_revision.wrapping_add(1);
                            if self.status.recovery_notice.is_empty() {
                                self.status.toasts.push(SAVED, ToastKind::Success);
                                self.status.message.clear();
                            } else {
                                self.status.toasts.push_for(
                                    SAVED_WITH_RECOVERY,
                                    ToastKind::Info,
                                    TOAST_LONG_SECS,
                                );
                                self.status.message =
                                    format!("{RECOVERY_COPIES} {}", self.status.recovery_notice);
                            }
                            self.status.message_error = false;
                        }
                        self.finish_save_intent(intent);
                    }
                    Err(error) => {
                        self.status.feedback_revision =
                            self.status.feedback_revision.wrapping_add(1);
                        self.status.message =
                            format!("Save failed: {error}. Retry saving before closing.");
                        self.status.message_error = true;
                        self.save_failed(intent, error);
                    }
                }
            }
            RuntimeNotice::SaveSkipped { intent } => {
                self.status.save_pending = self.runtime.user_save_pending();
                self.status.feedback_revision = self.status.feedback_revision.wrapping_add(1);
                self.status.message = "Stress review: nothing is saved".into();
                self.status.message_error = false;
                self.finish_save_intent(intent);
            }
            RuntimeNotice::SettingsNotSaved(error) => {
                self.status.feedback_revision = self.status.feedback_revision.wrapping_add(1);
                self.status.message = format!(
                    "Campaign saved; settings not saved: {error}. Settings retry on the next save."
                );
                self.status.message_error = true;
            }
            RuntimeNotice::SaveFolderOpened(result) => {
                self.status.storage_note = match result {
                    Ok(()) => String::new(),
                    Err(error) => format!(
                        "The folder could not be opened ({}). Use Copy path.",
                        error.trim_end_matches('.')
                    ),
                };
            }
            RuntimeNotice::ProfileReloaded(result) => self.profile_reloaded(result),
            RuntimeNotice::RecoveryCopies(paths) => {
                self.status.feedback_revision = self.status.feedback_revision.wrapping_add(1);
                self.status.recovery_notice = paths
                    .iter()
                    .map(|path| path.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", ");
                self.status.message = format!(
                    "Original unreadable save bytes preserved: {}",
                    self.status.recovery_notice
                );
                self.status.message_error = false;
            }
            RuntimeNotice::SettlementIssues(issues) => {
                self.status.feedback_revision = self.status.feedback_revision.wrapping_add(1);
                self.status.message = issues
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(" ");
                self.status.message_error = true;
            }
            RuntimeNotice::SimulationFinished => {
                self.campaign.mode = self
                    .game
                    .last_result()
                    .map_or(self.campaign.mode, |r| r.mode);
                self.focus_next_upgrade();
                self.go(Screen::Results);
                self.battle.fire_latched = false;
                self.runtime.clear_input();
            }
        }
    }

    /// A save of `intent` failed: the storage modal opens (UX-08), taking
    /// the slot from any open modal. A save a confirmed new journey replaced
    /// is not a storage failure. A failed autosave neither takes the modal
    /// from a failed save the player asked for (that modal's Retry keeps its
    /// intent) nor reopens an error the player already dismissed.
    fn save_failed(&mut self, intent: SaveIntent, error: String) {
        if error == SAVE_SUPERSEDED {
            return;
        }
        let autosave = intent == SaveIntent::Autosave;
        let asked_for_open = matches!(
            &self.status.modal,
            Some(AppModal::Storage(StorageError::Save { intent, .. }))
                if *intent != SaveIntent::Autosave
        );
        let dismissed = self.status.dismissed_save_error.as_deref() == Some(error.as_str());
        if autosave && (asked_for_open || dismissed) {
            return;
        }
        self.status.dismissed_save_error = None;
        self.status
            .open_storage(StorageError::Save { intent, error });
        // The modal takes the keyboard, so a live defense must not run on
        // behind it: pause through the runtime, as the menu routes do.
        if self.screen == Screen::Battle && self.game.attempt().is_some_and(|a| !a.paused) {
            self.dispatch(Command::SetPaused(true), "");
            self.battle.fire_latched = false;
            self.runtime.clear_input();
        }
    }

    /// `AppCommand::ReloadProfile` finished. On success the journey read from
    /// the save replaced the in-memory one, so the selections startup derives
    /// from the game are derived again; on failure the load modal shows the
    /// new error.
    fn profile_reloaded(&mut self, result: Result<(), String>) {
        self.status.feedback_revision = self.status.feedback_revision.wrapping_add(1);
        let open = match self.status.modal.take() {
            Some(AppModal::Storage(StorageError::Load { settings, .. })) => Some(settings),
            other => {
                self.status.modal = other;
                None
            }
        };
        match result {
            Ok(()) => {
                self.status.storage_note.clear();
                // The settings warning of the same startup still stands.
                if let Some(Some(warning)) = open {
                    self.status.modal = Some(AppModal::Storage(StorageError::Settings { warning }));
                }
                let campaign = self.game.campaign();
                self.status.profile_exists = campaign.selected_survivor.is_some()
                    || campaign.attempts > 0
                    || campaign.world.start.is_some();
                self.campaign.selected_country = campaign
                    .world
                    .active
                    .clone()
                    .unwrap_or_else(|| "DEU".into());
                self.campaign.preview_survivor =
                    campaign.selected_survivor.unwrap_or(SurvivorId::Rook);
                self.campaign.stage = 0;
                self.campaign.observed = None;
                self.campaign.improvement_focus = None;
                self.armory.selected_item = None;
                self.armory.pending_sale = None;
                if let Some(a) = self.game.attempt() {
                    self.battle.aim = [a.width_yards * 0.5, a.defender_position()[1]];
                    self.campaign.mode = a.mode;
                }
                self.status.message = crate::profile::CAMPAIGN_RESTORED.into();
                self.status.message_error = false;
            }
            Err(error) => {
                self.status.message = format!("Save could not be read: {error}.");
                self.status.message_error = true;
                if let Some(settings) = open {
                    self.status
                        .open_storage(StorageError::Load { error, settings });
                }
            }
        }
    }

    fn command_succeeded(&mut self, command: AppCommand, outcome: AppOutcome, success: String) {
        if matches!(
            command,
            AppCommand::Game(
                Command::BuyItem(_)
                    | Command::Equip { .. }
                    | Command::BuySkillNode(_)
                    | Command::TrainNpc(_)
                    | Command::BuyNpcPoint
            )
        ) {
            self.campaign.improvement_focus = None;
        }
        if let (
            AppCommand::Game(Command::BuyItem(_)),
            AppOutcome::Command(CommandOutcome::Id(id)),
        ) = (&command, &outcome)
        {
            self.armory.selected_item = Some(*id);
        }
        if !success.is_empty() {
            self.status.feedback_revision = self.status.feedback_revision.wrapping_add(1);
            self.status.message = match &outcome {
                AppOutcome::Command(CommandOutcome::Amount(amount)) => format!(
                    "{success} +{amount} {}",
                    if matches!(
                        command,
                        AppCommand::Game(Command::Salvage(_) | Command::SalvageExpedition { .. })
                    ) {
                        "supply"
                    } else {
                        "coins"
                    }
                ),
                _ => success,
            };
            self.status.message_error = false;
        }
        match command {
            AppCommand::NewJourney => {
                self.menu.new_journey_confirm = false;
                self.status.message_error = false;
                self.campaign.selected_country = "DEU".into();
                self.campaign.stage = 0;
                self.campaign.mode = Mode::Personal;
                self.campaign.preview_survivor = SurvivorId::Rook;
                self.campaign.improvement_focus = None;
                self.armory.selected_item = None;
                self.armory.pending_sale = None;
                self.battle.loot_feedback = loot::Feedback::default();
                self.battle.damage_feedback = damage::Feedback::default();
                self.campaign.observed = None;
                self.status.profile_exists = true;
                self.battle.fire_latched = false;
                self.battle.set_construction(false);
                self.battle.speed = 1;
                self.status.message = match outcome {
                    AppOutcome::JourneyArchived(Some(path)) => format!(
                        "New journey ready. Previous journey archived at {}",
                        path.display()
                    ),
                    _ => "New journey ready. Choose your survivor.".into(),
                };
                self.open_survivor_selection();
            }
            AppCommand::Game(Command::ContinueJourney) => {
                if matches!(
                    outcome,
                    AppOutcome::Command(CommandOutcome::CountryLiberated)
                ) {
                    // Name the liberation and preselect the easiest front it
                    // opened, so the map's next step is already chosen.
                    let freed = self.game.last_result().and_then(|r| r.country_id.clone());
                    let fronts = freed
                        .as_deref()
                        .map(|id| self.game.campaign().world.fronts_opened_by(id))
                        .unwrap_or_default();
                    if let Some(first) = fronts.first() {
                        self.campaign.selected_country = first.id.clone();
                    }
                    self.status.message = self.game.last_result().map_or_else(String::new, |r| {
                        horde_core::world::liberation_line(&r.country_name, fronts.len())
                    });
                    self.open_campaign();
                } else {
                    self.start_battle();
                }
            }
            AppCommand::Game(Command::ChooseSurvivor(id)) => {
                self.campaign.preview_survivor = id;
                self.status.profile_exists = true;
                self.open_campaign();
            }
            AppCommand::Game(Command::Deploy { .. }) => {
                self.status.profile_exists = true;
                self.start_battle();
            }
            AppCommand::Game(Command::Withdraw) => {
                self.campaign.mode = self
                    .game
                    .last_result()
                    .map_or(self.campaign.mode, |r| r.mode);
                self.go(Screen::Results);
                self.focus_next_upgrade();
                self.go_tab(Tab::Skills);
                self.nav.last_prepare_tab = Tab::Skills;
                self.battle.fire_latched = false;
                self.runtime.clear_input();
            }
            AppCommand::Game(Command::ChooseStart(id)) => {
                self.campaign.selected_country = id;
                self.campaign.stage = 0;
                self.status.profile_exists = true;
                self.start_battle();
            }
            AppCommand::Game(Command::EnterCountry(id)) => {
                self.campaign.selected_country = id;
                self.campaign.stage = self.game.campaign().world.next_stage();
                self.status.profile_exists = true;
                if self.game.attempt().is_some() {
                    self.start_battle();
                } else {
                    self.open_preparation(Tab::Briefing);
                }
            }
            AppCommand::Game(Command::SelectCountry(id)) => {
                // The country is chosen, nothing is deployed (CD6): open its
                // Briefing on the stage to plan. An outbreak's response
                // replays the final stage (`Game::outbreak_plan`).
                self.status.profile_exists = true;
                self.open_preparation(Tab::Briefing);
                self.campaign.stage = self.game.outbreak_plan(&id).map_or_else(
                    || self.game.campaign().world.next_stage(),
                    |plan| plan.stage,
                );
                self.campaign.selected_country = id;
            }
            AppCommand::Game(Command::SetPaused(_)) => {
                self.battle.fire_latched = false;
                self.runtime.clear_input();
            }
            AppCommand::Game(Command::SellItem(_)) => {
                self.armory.pending_sale = None;
                self.armory.selected_item = None;
            }
            _ => {}
        }
    }
}
