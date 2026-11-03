use crate::{
    art::{self, AMBER, GREEN, INK, MUTED, PANEL, RED, TEXT},
    runner::{
        self, AppCommand, AppOutcome, AttackAims, ControlFrame, Runtime, RuntimeHost,
        RuntimeNotice, SaveIntent,
    },
    settings::{self, Action, Settings},
    worldmap::MapCache,
};
use bevy::prelude::*;
use bevy_egui::{
    EguiContexts,
    egui::{
        self, Align, Align2, Color32, FontId, Layout, Rect, RichText, Sense, Stroke, pos2, vec2,
    },
};
use horde_core::{
    campaign::{BodyAttribute, NpcTraining, Skill},
    command::{Command, CommandOutcome, MAX_RECALL_PERCENT, NpcTactics},
    content::{Mode, Policy, Slot, StructureKind, content},
    ordnance::{OrdnanceKind, ThrowRequest},
    sim::{Game, Input, Outcome},
    survivors::{Appearance, SurvivorId},
    world::world,
};
use std::path::PathBuf;

#[cfg(test)]
#[path = "ui_tests.rs"]
mod tests;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Menu,
    SurvivorSelection,
    Camp,
    Battle,
    Results,
    History,
    Settings,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    World,
    Briefing,
    Character,
    Survivor,
    Armory,
    Skills,
    Engineering,
    Operations,
}

#[derive(Resource)]
pub struct Frontend {
    pub game: Game,
    pub runtime: Runtime,
    textures: crate::visual_assets::ArtTextures,
    pub screen: Screen,
    settings: Settings,
    balance_draft: Option<horde_core::balance::BalanceSettings>,
    history: history::HistoryState,
    history_return: Screen,
    history_world_paused: bool,
    return_screen: Screen,
    return_world_paused: bool,
    tab: Tab,
    last_prepare_tab: Tab,
    improvement_focus: Option<horde_core::campaign::RecommendationAction>,
    mode: Mode,
    stage: usize,
    selected_country: String,
    preview_survivor: SurvivorId,
    search: String,
    map: MapCache,
    message: String,
    message_error: bool,
    recovery_notice: String,
    feedback: feedback::FeedbackState,
    feedback_revision: u64,
    practice_profile: bool,
    save_pending: bool,
    profile_exists: bool,
    selected_item: Option<u64>,
    armory_search: String,
    armory_slot: Option<Slot>,
    armory_quality: Option<horde_core::quality::Quality>,
    armory_sort: armory::ArmorySort,
    pending_sale: Option<u64>,
    save_path: PathBuf,
    settings_path: PathBuf,
    initialized: bool,
    construction: bool,
    build_kind: StructureKind,
    attack_selection: battle::AttackSelection,
    loot_feedback: loot::Feedback,
    damage_feedback: damage::Feedback,
    fire_latched: bool,
    aim: [f32; 2],
    rebind: Option<Action>,
    inspect_log: bool,
    speed: u32,
    preset_name: String,
    sell_confirm: bool,
    new_journey_confirm: bool,
    pending: Input,
    pending_attack_aims: AttackAims,
    arena_rect: Option<Rect>,
    effects_rect: Option<Rect>,
    binding_armed: bool,
    observed: Option<u64>,
    arena_hover: bool,
    custom_cursor_requested: bool,
    last_control_frame: u64,
    last_toggle_frame: u64,
}
impl Frontend {
    pub fn load() -> Self {
        let practice = crate::environment::flag("LASTLIGHT_PRACTICE");
        let scenario = crate::environment::string("LASTLIGHT_REVIEW_SCREEN");
        let directory =
            runner::profile_directory(settings::save_directory(), scenario.as_deref(), practice);
        let loaded = runner::load_profile(directory);
        let profile_exists = loaded.game.campaign.selected_survivor.is_some()
            || loaded.game.campaign.attempts > 0
            || loaded.game.campaign.world.start.is_some();
        let runner::LoadedProfile {
            game,
            settings,
            campaign_path: save_path,
            settings_path,
            warning: message,
        } = loaded;
        let selected_country = game
            .campaign
            .world
            .active
            .clone()
            .unwrap_or_else(|| "DEU".into());
        let initial_aim = [
            game.attempt.as_ref().map_or_else(
                || game.campaign.arena_geometry().1 * 0.5,
                |a| a.width_yards * 0.5,
            ),
            game.attempt.as_ref().map_or_else(
                || f32::from(game.campaign.lane_count()) * 0.5,
                |a| a.defender_position()[1],
            ),
        ];
        let preview_survivor = game.campaign.selected_survivor.unwrap_or(SurvivorId::Rook);
        let mut f = Self {
            game,
            runtime: Runtime::default(),
            textures: crate::visual_assets::ArtTextures::default(),
            screen: Screen::Menu,
            settings,
            balance_draft: None,
            history: history::HistoryState::default(),
            history_return: Screen::Menu,
            history_world_paused: true,
            return_screen: Screen::Menu,
            return_world_paused: false,
            tab: Tab::World,
            last_prepare_tab: Tab::Briefing,
            improvement_focus: None,
            mode: Mode::Personal,
            stage: 0,
            selected_country,
            preview_survivor,
            search: String::new(),
            map: MapCache::default(),
            message,
            message_error: false,
            recovery_notice: String::new(),
            feedback: feedback::FeedbackState::default(),
            feedback_revision: 0,
            practice_profile: practice || scenario.is_some(),
            save_pending: false,
            profile_exists,
            selected_item: None,
            armory_search: String::new(),
            armory_slot: None,
            armory_quality: None,
            armory_sort: armory::ArmorySort::Newest,
            pending_sale: None,
            save_path,
            settings_path,
            initialized: false,
            construction: false,
            build_kind: StructureKind::Trap,
            attack_selection: battle::AttackSelection::Gun,
            loot_feedback: loot::Feedback::default(),
            damage_feedback: damage::Feedback::default(),
            fire_latched: false,
            aim: initial_aim,
            rebind: None,
            inspect_log: false,
            speed: 1,
            preset_name: "Field loadout".into(),
            sell_confirm: false,
            new_journey_confirm: false,
            pending: Input::default(),
            pending_attack_aims: AttackAims::default(),
            arena_rect: None,
            effects_rect: None,
            binding_armed: false,
            observed: None,
            arena_hover: false,
            custom_cursor_requested: false,
            last_control_frame: u64::MAX,
            last_toggle_frame: u64::MAX,
        };
        if !(practice && profile_exists) {
            review::prepare(&mut f);
        } else {
            f.message = "Practice scenario - saved progress is isolated from your campaign".into();
        }
        review::apply_reduced_override(
            &mut f,
            scenario.is_some(),
            practice,
            crate::environment::flag("LASTLIGHT_REVIEW_REDUCED"),
        );
        // Explicit automated-motion fixtures use a separate temporary profile.
        // This changes startup state only; fixed systems still own every tick.
        if scenario.is_some() && !practice && crate::environment::flag("LASTLIGHT_REVIEW_MOTION") {
            f.game.set_paused(false);
            f.construction = false;
            f.message = "Live motion review · progress is isolated from your campaign".into();
        }
        f
    }
    pub fn ambience_target(&self) -> crate::atmosphere_audio::SceneAudioTarget {
        use crate::atmosphere_audio::{AudioContext, SceneAudioTarget};
        let observed = if self.screen == Screen::Camp && self.tab == Tab::Operations {
            self.observed
                .and_then(|id| {
                    self.game
                        .battles
                        .iter()
                        .find(|battle| battle.id == id && !battle.settled)
                })
                .and_then(|battle| battle.game.attempt.as_ref())
        } else {
            None
        };
        let attempt = self.game.attempt.as_ref().or(observed);
        let theme = attempt
            .and_then(|attempt| {
                world()
                    .country(attempt.country_id.as_deref().unwrap_or(""))
                    .map(|country| country.stage_theme(attempt.stage))
            })
            .or_else(|| {
                world()
                    .country(&self.selected_country)
                    .or_else(|| self.game.campaign.world.active_country())
                    .map(|country| country.stage_theme(self.stage))
            });
        let context = if matches!(self.screen, Screen::Menu | Screen::SurvivorSelection) {
            AudioContext::Menu
        } else if self.screen == Screen::Settings
            || self.screen == Screen::Results
            || self.game.global_paused
            || attempt.is_some_and(|attempt| attempt.paused)
        {
            AudioContext::Paused
        } else if attempt.is_some() {
            AudioContext::Battle
        } else {
            AudioContext::Camp
        };
        SceneAudioTarget::new(
            theme.as_ref(),
            attempt.map(|attempt| attempt.mode),
            self.settings.master,
            self.settings.ambience,
            context,
        )
    }
    pub(crate) fn custom_cursor_requested(&self) -> bool {
        self.custom_cursor_requested
            && matches!(self.screen, Screen::Battle | Screen::Camp)
            && !self.feedback.details_open
    }
    pub fn request_save(&mut self, intent: SaveIntent) {
        if matches!(
            intent,
            SaveIntent::Menu | SaveIntent::Quit | SaveIntent::SettingsBack
        ) {
            self.runtime.enqueue(Command::SetPaused(true), "");
        }
        self.runtime.enqueue(AppCommand::Save(intent), "");
        self.save_pending = true;
        self.feedback_revision = self.feedback_revision.wrapping_add(1);
        self.message = "Saving campaign and current actions…".into();
        self.message_error = false;
    }
    fn settings_return_label(&self) -> &'static str {
        match self.return_screen {
            Screen::Menu | Screen::Settings => "Save and return to main menu",
            Screen::SurvivorSelection => "Save and return to survivor selection",
            Screen::Camp => "Save and return to camp",
            Screen::Battle => "Save and return to defense",
            Screen::Results => "Save and return to journey upgrades",
            Screen::History => "Save and return to run history",
        }
    }
    fn open_history(&mut self) {
        self.history_return = self.screen;
        self.history_world_paused = self.game.global_paused;
        self.fire_latched = false;
        self.dispatch(Command::SetPaused(true), "");
        self.screen = Screen::History;
    }
    fn close_history(&mut self) {
        self.screen = self.history_return;
        self.dispatch(Command::SetPaused(self.history_world_paused), "");
    }
    fn open_settings(&mut self) {
        self.balance_draft = Some(self.game.campaign.balance_settings.clone());
        self.return_screen = self.screen;
        self.return_world_paused = self.game.global_paused;
        self.screen = Screen::Settings;
        self.dispatch(Command::SetPaused(true), "");
        self.fire_latched = false;
        self.runtime.clear_input();
    }
    fn save_settings_to_menu(&mut self) {
        // Pause through the runtime before the save commits. Returning to the
        // menu must preserve active actions and freeze every live field.
        self.dispatch(Command::SetPaused(true), "");
        self.fire_latched = false;
        self.runtime.clear_input();
        self.request_save(SaveIntent::Menu);
    }
    fn dispatch(&mut self, command: impl Into<AppCommand>, success: &str) {
        let command = command.into();
        let reset = matches!(command, AppCommand::NewJourney);
        if self.runtime.enqueue(command, success) && reset {
            self.save_pending = true;
        }
    }
    fn request_new_game(&mut self) {
        if self.runtime.reset_in_progress() {
            return;
        }
        if self.profile_exists
            || self.save_path.try_exists().unwrap_or(true)
            || self.game.campaign.selected_survivor.is_some()
            || self.game.campaign.world.start.is_some()
        {
            self.new_journey_confirm = true;
        } else {
            self.dispatch(AppCommand::NewJourney, "New journey ready.");
        }
    }
    fn toggle_fire(&mut self, ctx: &egui::Context) {
        let frame = ctx.cumulative_frame_nr();
        if self.last_toggle_frame != frame {
            self.last_toggle_frame = frame;
            self.fire_latched = !self.fire_latched;
        }
    }
    fn begin(&mut self) {
        self.dispatch(
            Command::Deploy {
                stage: self.stage,
                mode: self.mode,
            },
            "Defense underway. Rewards stay banked if you retreat.",
        );
    }
    fn operations_available(&self) -> bool {
        use horde_core::skill_catalog::CampaignFeature;
        [
            CampaignFeature::Recruitment,
            CampaignFeature::Parallel,
            CampaignFeature::Operations,
        ]
        .into_iter()
        .any(|feature| self.game.campaign.has_feature(feature))
            || !self.game.strategy.roster.is_empty()
            || !self.game.battles.is_empty()
    }
    fn focus_next_upgrade(&mut self) {
        let suggested = self.game.campaign.recommendation(self.mode).action;
        self.improvement_focus = match suggested {
            horde_core::campaign::RecommendationAction::LearnNode(_)
            | horde_core::campaign::RecommendationAction::LearnSkill(_) => Some(suggested),
            _ => horde_core::skill_catalog::skills()
                .nodes
                .iter()
                .find(|n| {
                    !self.game.campaign.has_node(&n.id)
                        && self.game.campaign.can_purchase_node(&n.id).is_ok()
                })
                .or_else(|| {
                    horde_core::skill_catalog::skills().nodes.iter().find(|n| {
                        !self.game.campaign.has_node(&n.id)
                            && n.prerequisites
                                .iter()
                                .all(|id| self.game.campaign.has_node(id))
                    })
                })
                .map(|n| horde_core::campaign::RecommendationAction::LearnNode(n.id.clone())),
        };
    }
    fn open_upgrades(&mut self) {
        if self.game.attempt.is_some() {
            self.open_preparation(Tab::Skills);
            return;
        }
        self.dispatch(Command::SetPaused(true), "");
        self.screen = Screen::Results;
        self.fire_latched = false;
        self.runtime.clear_input();
    }
    fn open_preparation(&mut self, tab: Tab) {
        if matches!(tab, Tab::Skills | Tab::Engineering)
            && self.game.attempt.is_none()
            && self.game.campaign.world.start.is_some()
        {
            self.open_upgrades();
            return;
        }

        self.preserve_debrief_pause();
        if let Some(active) = self.game.campaign.world.active.clone() {
            self.selected_country = active;
        }
        self.screen = Screen::Camp;
        self.tab = tab;
        self.last_prepare_tab = tab;
        self.fire_latched = false;
        self.runtime.clear_input();
    }
    fn preserve_debrief_pause(&mut self) {
        if self.screen == Screen::Results && !self.game.global_paused {
            self.dispatch(Command::SetPaused(true), "");
        }
    }
    fn open_campaign(&mut self) {
        self.preserve_debrief_pause();
        self.screen = Screen::Camp;
        self.tab = Tab::World;
        self.fire_latched = false;
        self.runtime.clear_input();
    }
    fn open_briefing(&mut self) {
        if let Some(country) = self.game.campaign.world.active.clone() {
            self.selected_country = country;
            self.stage = self.game.campaign.world.next_stage();
        }
        self.open_preparation(Tab::Briefing);
    }
    fn open_survivor_selection(&mut self) {
        self.screen = Screen::SurvivorSelection;
        self.preview_survivor = self
            .game
            .campaign
            .selected_survivor
            .unwrap_or(self.preview_survivor);
        self.fire_latched = false;
        self.runtime.clear_input();
    }
    fn resume(&mut self) {
        if self.game.campaign.world.start.is_none()
            && self.game.campaign.selected_survivor.is_none()
        {
            self.open_survivor_selection();
            return;
        }
        self.screen = if self.game.attempt.is_some() {
            Screen::Battle
        } else if self.game.campaign.world.start.is_some() && self.game.campaign.journey_active {
            self.focus_next_upgrade();
            Screen::Results
        } else {
            self.tab = Tab::World;
            Screen::Camp
        };
        self.fire_latched = false;
        self.runtime.clear_input();
        if self.game.attempt.is_some() {
            self.dispatch(Command::SetPaused(true), "");
        }
    }
}
impl RuntimeHost for Frontend {
    fn runtime_parts(&mut self) -> (&mut Game, &mut Runtime) {
        (&mut self.game, &mut self.runtime)
    }
    fn runtime_observed_battle(&self) -> Option<u64> {
        if self.screen == Screen::Camp && self.tab == Tab::Operations {
            self.observed
        } else {
            None
        }
    }
    fn runtime_settings(&self) -> &Settings {
        &self.settings
    }
    fn runtime_paths(&self) -> (PathBuf, PathBuf) {
        (self.save_path.clone(), self.settings_path.clone())
    }
    fn runtime_notice(&mut self, notice: RuntimeNotice) {
        if matches!(
            &notice,
            RuntimeNotice::CommandCompleted {
                command: AppCommand::NewJourney,
                ..
            }
        ) {
            self.save_pending = self.runtime.user_save_pending();
        }
        match notice {
            RuntimeNotice::CommandCompleted {
                command,
                result,
                success,
            } => match result {
                Ok(outcome) => {
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
                        self.improvement_focus = None;
                    }
                    if let (
                        AppCommand::Game(Command::BuyItem(_)),
                        AppOutcome::Command(CommandOutcome::Id(id)),
                    ) = (&command, &outcome)
                    {
                        self.selected_item = Some(*id);
                    }
                    if !success.is_empty() {
                        self.feedback_revision = self.feedback_revision.wrapping_add(1);
                        self.message = match &outcome {
                            AppOutcome::Command(CommandOutcome::Amount(amount)) => format!(
                                "{success} +{amount} {}",
                                if matches!(
                                    command,
                                    AppCommand::Game(
                                        Command::Salvage(_) | Command::SalvageExpedition { .. }
                                    )
                                ) {
                                    "supply"
                                } else {
                                    "coins"
                                }
                            ),
                            _ => success,
                        };
                        self.message_error = false;
                    }
                    match command {
                        AppCommand::NewJourney => {
                            self.new_journey_confirm = false;
                            self.message_error = false;
                            self.selected_country = "DEU".into();
                            self.stage = 0;
                            self.mode = Mode::Personal;
                            self.preview_survivor = SurvivorId::Rook;
                            self.improvement_focus = None;
                            self.selected_item = None;
                            self.pending_sale = None;
                            self.loot_feedback = loot::Feedback::default();
                            self.damage_feedback = damage::Feedback::default();
                            self.observed = None;
                            self.profile_exists = true;
                            self.fire_latched = false;
                            self.construction = false;
                            self.speed = 1;
                            self.message = match outcome {
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
                                self.open_campaign();
                            } else {
                                self.screen = Screen::Battle;
                                self.mode = self
                                    .game
                                    .attempt
                                    .as_ref()
                                    .map_or(Mode::Personal, |a| a.mode);
                                if let Some(a) = &self.game.attempt {
                                    self.aim = [a.width_yards * 0.5, a.defender_position()[1]];
                                }
                                self.construction = false;
                                self.fire_latched = false;
                                self.speed = 1;
                                self.runtime.clear_input();
                            }
                        }
                        AppCommand::Game(Command::ChooseSurvivor(id)) => {
                            self.preview_survivor = id;
                            self.profile_exists = true;
                            self.open_campaign();
                        }
                        AppCommand::Game(Command::Deploy { .. }) => {
                            self.screen = Screen::Battle;
                            if let Some(a) = self.game.attempt.as_ref() {
                                self.aim = [a.width_yards * 0.5, a.defender_position()[1]];
                            }
                            self.construction = false;
                            self.attack_selection = battle::AttackSelection::Gun;
                            self.fire_latched = false;
                            self.speed = 1;
                            self.profile_exists = true;
                            self.runtime.clear_input();
                        }
                        AppCommand::Game(Command::Withdraw) => {
                            self.mode =
                                self.game.last_result.as_ref().map_or(self.mode, |r| r.mode);
                            self.screen = Screen::Results;
                            self.focus_next_upgrade();
                            self.tab = Tab::Skills;
                            self.last_prepare_tab = self.tab;
                            self.fire_latched = false;
                            self.runtime.clear_input();
                        }
                        AppCommand::Game(Command::ChooseStart(id)) => {
                            self.selected_country = id;
                            self.stage = 0;
                            self.mode = Mode::Personal;
                            if let Some(a) = self.game.attempt.as_ref() {
                                self.aim = [a.width_yards * 0.5, a.defender_position()[1]];
                            }
                            self.screen = Screen::Battle;
                            self.profile_exists = true;
                            self.construction = false;
                            self.attack_selection = battle::AttackSelection::Gun;
                            self.fire_latched = false;
                            self.runtime.clear_input();
                        }
                        AppCommand::Game(Command::EnterCountry(id)) => {
                            self.selected_country = id;
                            self.stage = self.game.campaign.world.next_stage();
                            self.profile_exists = true;
                            if self.game.attempt.is_some() {
                                self.screen = Screen::Battle;
                                self.mode = Mode::Personal;
                                if let Some(a) = &self.game.attempt {
                                    self.aim = [a.width_yards * 0.5, a.defender_position()[1]];
                                }
                            } else {
                                self.open_preparation(Tab::Briefing);
                            }
                        }
                        AppCommand::Game(Command::SetPaused(_)) => {
                            self.fire_latched = false;
                            self.runtime.clear_input();
                        }
                        AppCommand::Game(Command::SellItem(_)) => {
                            self.pending_sale = None;
                            self.selected_item = None;
                        }
                        _ => {}
                    }
                }
                Err(error) => {
                    self.feedback_revision = self.feedback_revision.wrapping_add(1);
                    self.message = error;
                    self.message_error = true;
                }
            },
            RuntimeNotice::Saved { intent, result } => {
                self.save_pending = self.runtime.user_save_pending();
                match result {
                    Ok(()) => {
                        self.profile_exists = true;
                        if intent != SaveIntent::Autosave {
                            self.feedback_revision = self.feedback_revision.wrapping_add(1);
                            self.message = if self.recovery_notice.is_empty() {
                                "Campaign and current actions saved.".into()
                            } else {
                                format!("Campaign saved. Recovery copies: {}", self.recovery_notice)
                            };
                            self.message_error = false;
                        }
                        match intent {
                            SaveIntent::Menu => {
                                self.screen = Screen::Menu;
                                self.rebind = None;
                                self.fire_latched = false;
                                self.runtime.clear_input();
                            }
                            SaveIntent::SettingsBack => {
                                self.screen = match self.return_screen {
                                    Screen::Settings => Screen::Menu,
                                    screen => screen,
                                };
                                self.rebind = None;
                                if self.return_screen == Screen::Camp {
                                    self.dispatch(Command::SetPaused(self.return_world_paused), "");
                                }
                            }
                            _ => {}
                        }
                    }
                    Err(error) => {
                        self.feedback_revision = self.feedback_revision.wrapping_add(1);
                        self.message =
                            format!("Save failed: {error}. Retry saving before closing.");
                        self.message_error = true;
                    }
                }
            }
            RuntimeNotice::RecoveryCopies(paths) => {
                self.feedback_revision = self.feedback_revision.wrapping_add(1);
                self.recovery_notice = paths
                    .iter()
                    .map(|path| path.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", ");
                self.message = format!(
                    "Original unreadable save bytes preserved: {}",
                    self.recovery_notice
                );
                self.message_error = false;
            }
            RuntimeNotice::SettlementIssues(issues) => {
                self.feedback_revision = self.feedback_revision.wrapping_add(1);
                self.message = issues
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(" ");
                self.message_error = true;
            }
            RuntimeNotice::SimulationFinished => {
                self.mode = self.game.last_result.as_ref().map_or(self.mode, |r| r.mode);
                self.focus_next_upgrade();
                self.screen = Screen::Results;
                self.fire_latched = false;
                self.runtime.clear_input();
            }
        }
    }
}

fn status(ui: &mut egui::Ui, f: &mut Frontend) {
    feedback::status_line(ui, f);
}

fn muted(ui: &mut egui::Ui, text: impl Into<String>) {
    ui.label(RichText::new(text).color(MUTED));
}
fn primary(ui: &mut egui::Ui, text: &str) -> bool {
    ui.add(
        egui::Button::new(RichText::new(text).color(TEXT).strong())
            .fill(Color32::BLACK)
            .stroke(Stroke::new(2.0, TEXT))
            .min_size(vec2(ui.available_width().min(190.0), 40.0))
            .wrap(),
    )
    .clicked()
}
fn progress(ui: &mut egui::Ui, value: f32, text: String, color: Color32) {
    ui.add(
        egui::ProgressBar::new(value.clamp(0.0, 1.0))
            .corner_radius(0)
            .fill(color.gamma_multiply(0.45))
            .text(text)
            .desired_height(18.0),
    );
}

/// Diagnostic metadata produced by the actual last rendered model review.
/// The screenshot service owns serialization and disk I/O.
pub(crate) fn player_model_review_report(ctx: &egui::Context) -> Option<review::NativeModelReport> {
    review::model_report(ctx)
}

pub fn draw(
    mut contexts: EguiContexts,
    mut f: ResMut<Frontend>,
    time: Res<Time>,
    frames: Res<bevy::diagnostic::FrameCount>,
    visual_assets: Res<crate::visual_assets::VisualAssets>,
    images: Res<Assets<Image>>,
    render_device: Res<bevy::render::renderer::RenderDevice>,
) -> Result {
    let _work = crate::diagnostics::work_timer(crate::diagnostics::WorkPhase::Interface);
    f.custom_cursor_requested = false;
    f.runtime.begin_frame(frames.0);
    f.textures = crate::visual_assets::register(
        &mut contexts,
        &visual_assets,
        &images,
        render_device.limits().max_texture_dimension_2d as usize,
    );
    let ctx = contexts.ctx_mut()?;
    if !f.initialized {
        let texture_limit = render_device.limits().max_texture_dimension_2d as usize;
        ctx.input_mut(|input| {
            input.max_texture_side = texture_limit;
            input.raw.max_texture_side = Some(texture_limit);
        });
        crate::visual_assets::prepare_player_models(ctx).map_err(std::io::Error::other)?;
        settings::install_font(ctx);
        f.initialized = true;
        ctx.request_repaint();
        return Ok(());
    }
    settings::configure(ctx, f.settings.scale);
    ctx.style_mut_of(egui::Theme::Dark, |style| {
        style.animation_time = if f.settings.reduced_effects {
            0.0
        } else {
            0.12
        }
    });
    let view_accent = feedback::view_progress(ctx, f.screen, f.settings.reduced_effects);
    let mut root_ui = egui::Ui::new(
        ctx.clone(),
        egui::Id::new("lastlight_root"),
        egui::UiBuilder::new()
            .max_rect(ctx.content_rect())
            .layer_id(egui::LayerId::background()),
    );
    let root = &mut root_ui;
    if f.screen == Screen::Battle {
        controls(ctx, &mut f);
    }
    if !matches!(
        f.screen,
        Screen::Menu | Screen::SurvivorSelection | Screen::Battle | Screen::Results
    ) && !(f.screen == Screen::Camp && matches!(f.tab, Tab::Skills | Tab::Engineering))
    {
        navigation::header(root, &mut f, view_accent);
    }
    navigation::field_status(root, &mut f);
    let previous_screen = f.screen;
    if review::model_workshop(root, &f) {
        publish_controls(ctx, &mut f, false, false);
        ctx.request_repaint();
        return Ok(());
    }
    match f.screen {
        Screen::Menu => menu(root, &mut f, time.elapsed_secs()),
        Screen::SurvivorSelection => menu::survivor_selection(root, &mut f, time.elapsed_secs()),
        Screen::Camp => camp(root, &mut f),
        Screen::Battle => battle(root, &mut f),
        Screen::Results => results(root, &mut f),
        Screen::History => history::show(root, &mut f),
        Screen::Settings => settings_ui(root, &mut f),
    }
    let active = f.screen == Screen::Battle
        && f.game.attempt.as_ref().is_some_and(|a| !a.paused)
        && previous_screen == Screen::Battle;
    let strategy_active =
        f.screen == Screen::Camp && f.game.attempt.is_none() && !f.game.global_paused;
    publish_controls(ctx, &mut f, active, strategy_active);
    ctx.request_repaint();
    Ok(())
}

fn publish_controls(ctx: &egui::Context, f: &mut Frontend, active: bool, strategy_active: bool) {
    let mut held = if active {
        gather_combat_input(ctx, f)
    } else {
        Input::default()
    };
    let mut pressed = std::mem::take(&mut f.pending);
    let pressed_aims = std::mem::take(&mut f.pending_attack_aims);
    pressed.reload |= held.reload;
    pressed.heal |= held.heal;
    pressed.brace |= held.brace;
    held.reload = false;
    held.heal = false;
    held.brace = false;
    let speed = if f
        .game
        .attempt
        .as_ref()
        .is_some_and(|a| a.mode == Mode::Personal)
    {
        1
    } else {
        f.speed
    };
    f.runtime.publish(ControlFrame {
        enabled: active || strategy_active,
        speed,
        held,
        pressed,
        pressed_aims,
    });
}

mod menu;
use self::menu::menu;
mod briefing;
mod camp;
mod chrome;
mod navigation;
use self::camp::camp;
#[cfg(test)]
use self::camp::country_dossier;
mod character;
use self::character::{character, mode_selector, policy, survivor};
mod armory;
use self::armory::armory;
mod progression;
use self::progression::{engineering, skills};
mod battle;
mod damage;
mod loot;
mod reticle;
use self::battle::{battle, controls, gather_combat_input};
mod results;
use self::results::results;
mod history;
mod preferences;
use self::preferences::settings_ui;
mod operations;
use self::operations::operations;

mod review;

mod feedback;

mod hud;
