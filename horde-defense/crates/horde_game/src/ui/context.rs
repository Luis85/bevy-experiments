//! `UiCtx`: what one widget pass may touch. It reads the game, queues commands
//! on the runtime, edits UI state and requests state transitions; it cannot
//! mutate `GameSession`. `UiParams` builds it from App resources.
use super::{
    ArmoryUi, BattleUi, CampaignUi, MenuUi, Navigation, Screen, ScreenAmbience, SettingsUi,
    StatusUi, Tab, history::HistoryUi,
};
use crate::{
    atmosphere_audio::{AudioContext, SceneAudioTarget},
    audio::Cue as SoundCue,
    launch::LaunchConfig,
    profile::ProfilePaths,
    runner::{AppCommand, GameSession, Runtime, SaveIntent},
    settings::Settings,
    visual_assets::ArtTextures,
};
use bevy::{ecs::system::SystemParam, prelude::*};
use bevy_egui::egui;
use horde_core::{campaign::UpgradeFocus, command::Command, sim::Game, world::world};

pub(crate) struct UiCtx<'a> {
    pub(super) game: &'a Game,
    pub(super) runtime: &'a mut Runtime,
    pub(super) settings: &'a mut Settings,
    pub(super) paths: &'a ProfilePaths,
    pub(super) launch: &'a LaunchConfig,
    pub(super) textures: ArtTextures,
    /// The screen and camp tab applied for this pass. Requests made during the
    /// pass take effect at the next state transition.
    pub(super) screen: Screen,
    pub(super) tab: Tab,
    pub(super) next_screen: &'a mut NextState<Screen>,
    pub(super) next_tab: &'a mut NextState<Tab>,
    pub(super) nav: &'a mut Navigation,
    pub(super) status: &'a mut StatusUi,
    pub(super) campaign: &'a mut CampaignUi,
    pub(super) armory: &'a mut ArmoryUi,
    pub(super) battle: &'a mut BattleUi,
    pub(super) history: &'a mut HistoryUi,
    pub(super) prefs: &'a mut SettingsUi,
    pub(super) menu: &'a mut MenuUi,
    /// UI-originated sound cues, forwarded as `SoundCue` messages.
    pub(super) cues: &'a mut Vec<SoundCue>,
    /// Perf launches only: sub-spans and scene counts inside `ui::draw`.
    pub(super) perf: Option<&'a crate::diagnostics::PerfProbe>,
    /// Whether this process started with pipelined rendering
    /// (`RenderPlan::pipelined`). It never changes live, so Settings marks
    /// Low latency "(applies after restart)" while the setting asks for the
    /// other state.
    pub(super) plan_pipelined: bool,
}

/// Per-screen UI state resources, grouped to keep `UiParams` within Bevy's
/// system-parameter arity.
#[derive(SystemParam)]
pub(crate) struct ScreenState<'w> {
    nav: ResMut<'w, Navigation>,
    status: ResMut<'w, StatusUi>,
    campaign: ResMut<'w, CampaignUi>,
    armory: ResMut<'w, ArmoryUi>,
    battle: ResMut<'w, BattleUi>,
    history: ResMut<'w, HistoryUi>,
    prefs: ResMut<'w, SettingsUi>,
    menu: ResMut<'w, MenuUi>,
}

/// The App resources behind a `UiCtx`. `GameSession` is read-only here.
#[derive(SystemParam)]
pub(crate) struct UiParams<'w> {
    session: Res<'w, GameSession>,
    runtime: ResMut<'w, Runtime>,
    settings: ResMut<'w, Settings>,
    paths: Res<'w, ProfilePaths>,
    launch: Res<'w, LaunchConfig>,
    textures: Res<'w, ArtTextures>,
    screen: Res<'w, State<Screen>>,
    tab: Option<Res<'w, State<Tab>>>,
    next_screen: ResMut<'w, NextState<Screen>>,
    next_tab: ResMut<'w, NextState<Tab>>,
    state: ScreenState<'w>,
    /// Absent in a headless App without the display plugin.
    plan: Option<Res<'w, crate::render_plan::RenderPlan>>,
}
impl UiParams<'_> {
    pub(crate) fn ctx<'a>(&'a mut self, cues: &'a mut Vec<SoundCue>) -> UiCtx<'a> {
        let state = &mut self.state;
        // Without a plan nothing is pending a restart.
        let plan_pipelined = self.plan.as_ref().map_or_else(
            || crate::render_plan::pipelined_for(self.settings.low_latency, None),
            |plan| plan.pipelined,
        );
        UiCtx {
            game: &self.session.game,
            runtime: &mut self.runtime,
            settings: &mut self.settings,
            paths: &self.paths,
            launch: &self.launch,
            textures: *self.textures,
            screen: *self.screen.get(),
            tab: self.tab.as_ref().map_or(state.nav.tab, |tab| *tab.get()),
            next_screen: &mut self.next_screen,
            next_tab: &mut self.next_tab,
            nav: &mut state.nav,
            status: &mut state.status,
            campaign: &mut state.campaign,
            armory: &mut state.armory,
            battle: &mut state.battle,
            history: &mut state.history,
            prefs: &mut state.prefs,
            menu: &mut state.menu,
            cues,
            perf: None,
            plan_pipelined,
        }
    }
}

/// The status text while a save runs (design-ux-flow §4 "Save UX").
pub(super) const SAVING: &str = "Saving…";

/// Queues a save; menu, quit and settings-return saves pause first.
pub(crate) fn request_save(runtime: &mut Runtime, status: &mut StatusUi, intent: SaveIntent) {
    if matches!(
        intent,
        SaveIntent::Menu | SaveIntent::Quit | SaveIntent::SettingsBack
    ) {
        runtime.enqueue(Command::SetPaused(true), "");
    }
    runtime.enqueue(AppCommand::Save(intent), "");
    status.save_pending = true;
    status.feedback_revision = status.feedback_revision.wrapping_add(1);
    status.message = SAVING.into();
    status.message_error = false;
}

/// The ambience bed for the current screen, battle and settings.
pub(crate) fn ambience_target(
    game: &Game,
    screen: Screen,
    tab: Tab,
    campaign: &CampaignUi,
    settings: &Settings,
) -> SceneAudioTarget {
    let observed = if screen == Screen::Camp && tab == Tab::Operations {
        campaign
            .observed
            .and_then(|id| game.active_battle(id))
            .and_then(|battle| battle.game().attempt())
    } else {
        None
    };
    let attempt = game.attempt().or(observed);
    let theme = attempt
        .and_then(|attempt| {
            world()
                .country(attempt.country_id.as_deref().unwrap_or(""))
                .map(|country| country.stage_theme(attempt.stage))
        })
        .or_else(|| {
            world()
                .country(&campaign.selected_country)
                .or_else(|| game.campaign().world.active_country())
                .map(|country| country.stage_theme(campaign.stage))
        });
    let context = match screen.ambience() {
        ScreenAmbience::Menu => AudioContext::Menu,
        ScreenAmbience::Paused => AudioContext::Paused,
        ScreenAmbience::Live
            if game.global_paused() || attempt.is_some_and(|attempt| attempt.paused) =>
        {
            AudioContext::Paused
        }
        ScreenAmbience::Live if attempt.is_some() => AudioContext::Battle,
        ScreenAmbience::Live => AudioContext::Camp,
    };
    SceneAudioTarget::new(
        theme.as_ref(),
        attempt.map(|attempt| attempt.mode),
        settings.master,
        settings.ambience,
        context,
    )
}

impl UiCtx<'_> {
    // ----- state transitions -----

    /// Requests `screen` at the next state transition. Entering the camp
    /// restores the remembered tab.
    /// Re-requesting the current screen (with nothing else pending) is a no-op
    /// rather than an identity transition, so live control keeps running.
    pub(super) fn go(&mut self, screen: Screen) {
        if screen != self.screen || !matches!(self.next_screen, NextState::Unchanged) {
            self.next_screen.set(screen);
        }
        if screen == Screen::Camp {
            self.next_tab.set(self.nav.tab);
        }
    }
    /// Requests camp tab `tab` (remembered while the camp is not shown).
    /// Leaving Home drops its one-shot highlight.
    pub(super) fn go_tab(&mut self, tab: Tab) {
        if tab != Tab::Home {
            self.nav.home_focus = None;
        }
        self.nav.tab = tab;
        self.next_tab.set(tab);
    }
    /// Whether a screen change is requested but not yet applied.
    pub(super) fn leaving(&self) -> bool {
        match &*self.next_screen {
            NextState::Pending(screen) | NextState::PendingIfDifferent(screen) => {
                *screen != self.screen
            }
            NextState::Unchanged => false,
        }
    }
    /// A new deployment began: show the battle; `OnEnter(Screen::Battle)` resets
    /// its controls.
    pub(super) fn start_battle(&mut self) {
        self.nav.battle_reset = true;
        // Always a transition, so `OnEnter(Screen::Battle)` runs the reset even
        // if a battle screen is already shown.
        self.next_screen.set(Screen::Battle);
    }

    // ----- queries -----

    #[cfg(test)]
    pub(super) fn ambience_target(&self) -> SceneAudioTarget {
        ambience_target(
            self.game,
            self.screen,
            self.tab,
            self.campaign,
            self.settings,
        )
    }
    /// The expedition the operations tab is showing, if it is open.
    pub(super) fn observed_battle(&self) -> Option<u64> {
        if self.screen == Screen::Camp && self.tab == Tab::Operations {
            self.campaign.observed
        } else {
            None
        }
    }
    pub(super) fn settings_return_label(&self) -> &'static str {
        self.nav.return_screen.settings_return_label()
    }
    /// The Operations page is reachable; the rule is core's
    /// [`Game::operations_available`].
    pub(super) fn operations_available(&self) -> bool {
        self.game.operations_available()
    }

    // ----- commands -----

    pub(super) fn request_save(&mut self, intent: SaveIntent) {
        request_save(self.runtime, self.status, intent);
    }
    pub(super) fn dispatch(&mut self, command: impl Into<AppCommand>, success: &str) {
        let command = command.into();
        let reset = matches!(command, AppCommand::NewJourney);
        if self.runtime.enqueue(command, success) && reset {
            self.status.save_pending = true;
        }
    }
    pub(super) fn notify_rare_loot(&mut self) {
        crate::runner::push_cue(self.cues, SoundCue::Ready);
    }
    pub(super) fn request_new_game(&mut self) {
        if self.runtime.reset_in_progress() {
            return;
        }
        if self.status.profile_exists
            || self.paths.campaign().try_exists().unwrap_or(true)
            || self.game.campaign().selected_survivor.is_some()
            || self.game.campaign().world.start.is_some()
        {
            self.menu.new_journey_confirm = true;
        } else {
            self.dispatch(AppCommand::NewJourney, "New journey ready.");
        }
    }
    pub(super) fn toggle_fire(&mut self, ctx: &egui::Context) {
        let frame = ctx.cumulative_frame_nr();
        if self.battle.last_toggle_frame != frame {
            self.battle.last_toggle_frame = frame;
            self.battle.fire_latched = !self.battle.fire_latched;
        }
    }
    /// The Briefing primary (ruling D16): `BeginDefense` deploys core's
    /// next defender on the planned stage. The UI names no mode.
    pub(super) fn begin(&mut self) {
        self.dispatch(
            Command::BeginDefense {
                stage: self.campaign.stage,
            },
            "Defense underway. Rewards stay banked if you retreat.",
        );
    }
    pub(super) fn focus_next_upgrade(&mut self) {
        self.campaign.improvement_focus = self
            .game
            .campaign()
            .recommendation_fallback(UpgradeFocus::Next(self.campaign.mode))
            .map(|r| r.action);
    }

    // ----- navigation -----

    pub(super) fn open_history(&mut self) {
        self.nav.history_return = self.screen;
        self.nav.history_world_paused = self.game.global_paused();
        self.battle.fire_latched = false;
        self.dispatch(Command::SetPaused(true), "");
        self.go(Screen::History);
    }
    pub(super) fn close_history(&mut self) {
        self.go(self.nav.history_return);
        self.dispatch(Command::SetPaused(self.nav.history_world_paused), "");
    }
    pub(super) fn open_settings(&mut self) {
        self.prefs.balance_draft = Some(self.game.campaign().balance_settings.clone());
        self.prefs.balance_custom_chosen = false;
        self.prefs.scale_draft = None;
        self.nav.return_screen = self.screen;
        self.nav.return_world_paused = self.game.global_paused();
        self.go(Screen::Settings);
        self.dispatch(Command::SetPaused(true), "");
        self.battle.fire_latched = false;
        self.runtime.clear_input();
    }
    pub(super) fn save_settings_to_menu(&mut self) {
        // Pause through the runtime before the save commits. Returning to the
        // menu must preserve active actions and freeze every live field.
        self.dispatch(Command::SetPaused(true), "");
        self.battle.fire_latched = false;
        self.runtime.clear_input();
        self.request_save(SaveIntent::Menu);
    }
    /// The Upgrades screen: the one route to the complete tree. A live main
    /// defense stays paused there and is reached from its primary action.
    pub(super) fn open_upgrades(&mut self) {
        self.dispatch(Command::SetPaused(true), "");
        self.go(Screen::Upgrades);
        self.battle.fire_latched = false;
        self.runtime.clear_input();
    }
    /// The debrief of the settled round, paused. Without a settled report
    /// there is nothing to show, so the request opens Camp home.
    pub(super) fn open_results(&mut self) {
        if self.game.last_result().is_none() {
            self.open_camp_home(None);
            return;
        }
        self.dispatch(Command::SetPaused(true), "");
        self.go(Screen::Results);
        self.battle.fire_latched = false;
        self.runtime.clear_input();
    }
    pub(super) fn open_preparation(&mut self, tab: Tab) {
        // The old Skills and Fortifications tabs redirect to the one home;
        // Fortifications frames the Fortification branch there.
        if matches!(tab, Tab::Skills | Tab::Engineering) {
            if tab == Tab::Engineering {
                self.campaign.improvement_focus = self
                    .game
                    .campaign()
                    .recommendation_fallback(UpgradeFocus::Fortification)
                    .map(|r| r.action);
            }
            self.open_upgrades();
            return;
        }

        self.preserve_debrief_pause();
        if let Some(active) = self.game.campaign().world.active.clone() {
            self.campaign.selected_country = active;
        }
        self.go_tab(tab);
        self.go(Screen::Camp);
        self.nav.last_prepare_tab = tab;
        self.battle.fire_latched = false;
        self.runtime.clear_input();
    }
    fn preserve_debrief_pause(&mut self) {
        if matches!(self.screen, Screen::Results | Screen::Upgrades) && !self.game.global_paused() {
            self.dispatch(Command::SetPaused(true), "");
        }
    }
    /// Camp home, highlighting `focus` there. Before the first start there
    /// is no home yet, so the request opens the world map.
    pub(super) fn open_camp_home(&mut self, focus: Option<super::HomeFocus>) {
        if self.game.campaign().world.start.is_none() {
            self.open_campaign();
            return;
        }
        self.preserve_debrief_pause();
        self.go_tab(Tab::Home);
        self.nav.home_focus = focus;
        self.go(Screen::Camp);
        self.battle.fire_latched = false;
        self.runtime.clear_input();
    }
    pub(super) fn open_campaign(&mut self) {
        self.preserve_debrief_pause();
        self.go_tab(Tab::World);
        self.go(Screen::Camp);
        self.battle.fire_latched = false;
        self.runtime.clear_input();
    }
    pub(super) fn open_briefing(&mut self) {
        if let Some(country) = self.game.campaign().world.active.clone() {
            self.campaign.selected_country = country;
            self.campaign.stage = self.game.campaign().world.next_stage();
        }
        self.open_preparation(Tab::Briefing);
    }
    pub(super) fn open_survivor_selection(&mut self) {
        self.go(Screen::SurvivorSelection);
        self.campaign.preview_survivor = self
            .game
            .campaign()
            .selected_survivor
            .unwrap_or(self.campaign.preview_survivor);
        self.battle.fire_latched = false;
        self.runtime.clear_input();
    }
    pub(super) fn resume(&mut self) {
        if self.game.campaign().world.start.is_none()
            && self.game.campaign().selected_survivor.is_none()
        {
            self.open_survivor_selection();
            return;
        }
        if self.game.attempt().is_some() {
            self.go(Screen::Battle);
        } else if self.game.campaign().world.start.is_some()
            && self.game.campaign().journey_active
            && self.game.last_result().is_some()
        {
            // A settled journey resumes on its debrief.
            self.focus_next_upgrade();
            self.go(Screen::Results);
        } else {
            // Camp home; before the first start the camp shows the world map.
            self.go_tab(Tab::Home);
            self.go(Screen::Camp);
        }
        self.battle.fire_latched = false;
        self.runtime.clear_input();
        if self.game.attempt().is_some() {
            self.dispatch(Command::SetPaused(true), "");
        }
    }
}
