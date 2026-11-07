//! Runtime notices become feedback text and state transitions here, in one
//! system; `OnEnter(Screen::Battle)` owns the reset a new deployment needs.
use super::{
    BattleUi, CampaignUi, Navigation, Screen, Tab, UiCtx, UiParams, battle::AttackSelection,
    damage, loot,
};
use crate::{
    audio::Cue as SoundCue,
    runner::{AppCommand, AppOutcome, GameSession, Runtime, RuntimeNotice, SaveIntent},
};
use bevy::prelude::*;
use horde_core::{
    command::{Command, CommandOutcome},
    content::Mode,
    sim::Game,
    survivors::SurvivorId,
};

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
                self.prefs.rebind = None;
                self.battle.fire_latched = false;
                self.runtime.clear_input();
            }
            SaveIntent::SettingsBack => {
                self.go(match self.nav.return_screen {
                    Screen::Settings => Screen::Menu,
                    screen => screen,
                });
                self.prefs.rebind = None;
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
                        if intent != SaveIntent::Autosave {
                            self.status.feedback_revision =
                                self.status.feedback_revision.wrapping_add(1);
                            self.status.message = if self.status.recovery_notice.is_empty() {
                                "Campaign and current actions saved.".into()
                            } else {
                                format!(
                                    "Campaign saved. Recovery copies: {}",
                                    self.status.recovery_notice
                                )
                            };
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
                    self.status.message = match (self.game.last_result(), fronts.len()) {
                        (Some(r), 0) => format!("{} liberated.", r.country_name),
                        (Some(r), n) => format!(
                            "{} liberated · {n} new {} open on the map.",
                            r.country_name,
                            if n == 1 { "front" } else { "fronts" }
                        ),
                        (None, _) => String::new(),
                    };
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
