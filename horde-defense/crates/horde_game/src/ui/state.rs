//! Frontend resources, one per concern, and `FrontendParts`: the owned set of
//! them assembled at startup (and by tests) before being inserted into the App.
use super::{
    Screen, Tab, UiCtx, armory::ArmorySort, battle::AttackSelection, damage,
    feedback::FeedbackState, history::HistoryUi, loot, notices,
};
use crate::{
    audio::Cue as SoundCue,
    launch::LaunchConfig,
    profile::{LoadedProfile, ProfilePaths, SettingsLoad},
    runner::{AttackAims, GameSession, Runtime, RuntimeNotice},
    settings::{Action, BindingId, ExtraAction, Settings},
    visual_assets::ArtTextures,
    worldmap::MapCache,
};
use bevy::prelude::*;
use bevy_egui::egui::Rect;
use horde_core::{
    campaign::RecommendationAction,
    content::{Mode, Slot, StructureKind},
    quality::Quality,
    sim::{Game, Input},
    survivors::SurvivorId,
};

/// Camp tab memory and the return stack for overlay screens. The live screen
/// and tab are Bevy states; `tab` remembers the camp tab while elsewhere.
#[derive(Resource)]
pub(crate) struct Navigation {
    pub(super) tab: Tab,
    pub(super) last_prepare_tab: Tab,
    pub(super) history_return: Screen,
    pub(super) history_world_paused: bool,
    pub(super) return_screen: Screen,
    pub(super) return_world_paused: bool,
    /// Set when a new deployment starts; `OnEnter(Screen::Battle)` consumes it.
    pub(super) battle_reset: bool,
    /// What Camp home highlights when it opens (`open_camp_home`).
    pub(super) home_focus: Option<HomeFocus>,
    /// The screen and camp tab the onboarding last checked for arrival
    /// (`onboarding::visit`).
    pub(super) onboarding_place: Option<(Screen, Tab)>,
}

/// What Camp home highlights when a route opens it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HomeFocus {
    /// The roster row of this recruit (a recruit who just joined, from the
    /// Recruit modal's "Back to camp").
    Recruit(u64),
}
impl Default for Navigation {
    fn default() -> Self {
        Self {
            tab: Tab::Home,
            last_prepare_tab: Tab::Briefing,
            history_return: Screen::Menu,
            history_world_paused: true,
            return_screen: Screen::Menu,
            return_world_paused: false,
            battle_reset: false,
            home_focus: None,
            onboarding_place: None,
        }
    }
}

/// The status line: feedback text, save progress and profile kind.
#[derive(Resource, Default)]
pub(crate) struct StatusUi {
    pub(super) message: String,
    pub(super) message_error: bool,
    pub(super) recovery_notice: String,
    pub(super) feedback: FeedbackState,
    pub(super) feedback_revision: u64,
    pub(super) save_pending: bool,
    /// Short non-blocking feedback (ui/toast.rs), oldest first.
    pub(super) toasts: super::toast::Toasts,
    /// The status message the battle last turned into a toast, with its
    /// revision and error flag (`toast::status_toast`).
    pub(super) toasted: Option<(String, u64, bool)>,
    /// An `AcknowledgeOutcomes` this UI queued that the runner may not have
    /// applied yet (`expedition_notices::deliver`).
    pub(super) outcome_ack_pending: bool,
    /// The one application modal slot (ui/modal.rs).
    pub(super) modal: Option<super::modal::AppModal>,
    /// The storage modal's note: "Path copied." or why the folder could not
    /// be opened. Cleared when the modal opens or closes.
    pub(super) storage_note: String,
    /// The save error the player dismissed: the same autosave failure does
    /// not reopen the modal until a save succeeds.
    pub(super) dismissed_save_error: Option<String>,
    pub(super) practice_profile: bool,
    pub(super) profile_exists: bool,
}

/// World map, briefing and survivor selection choices.
#[derive(Resource)]
pub(crate) struct CampaignUi {
    pub(super) mode: Mode,
    pub(super) stage: usize,
    pub(super) selected_country: String,
    pub(super) preview_survivor: SurvivorId,
    pub(super) search: String,
    pub(super) map: MapCache,
    pub(super) improvement_focus: Option<RecommendationAction>,
    /// Search text the tree applies once on its next draw (review scenes).
    pub(super) tree_search_seed: Option<String>,
    /// The expedition shown on the operations tab.
    pub(super) observed: Option<u64>,
}

#[derive(Resource)]
pub(crate) struct ArmoryUi {
    pub(super) selected_item: Option<u64>,
    pub(super) search: String,
    pub(super) slot: Option<Slot>,
    pub(super) quality: Option<Quality>,
    pub(super) sort: ArmorySort,
    pub(super) pending_sale: Option<u64>,
    pub(super) preset_name: String,
    pub(super) sell_confirm: bool,
    /// Open the Auto-sell panel and scroll it into view on the next
    /// Equipment frame, then clear: set by a route that leads to the panel
    /// (the equipment-autosell review scene).
    pub(super) reveal_auto_sell: bool,
}
impl Default for ArmoryUi {
    fn default() -> Self {
        Self {
            selected_item: None,
            search: String::new(),
            slot: None,
            quality: None,
            sort: ArmorySort::Newest,
            pending_sale: None,
            preset_name: "Field loadout".into(),
            sell_confirm: false,
            reveal_auto_sell: false,
        }
    }
}

/// Defense controls, arena layout and presentation feedback.
#[derive(Resource)]
pub(crate) struct BattleUi {
    pub(super) construction: bool,
    pub(super) build_kind: StructureKind,
    pub(super) attack_selection: AttackSelection,
    pub(super) loot_feedback: loot::Feedback,
    pub(super) damage_feedback: damage::Feedback,
    pub(super) fire_latched: bool,
    pub(super) aim: [f32; 2],
    pub(super) inspect_log: bool,
    pub(super) speed: u32,
    pub(super) pending: Input,
    pub(super) pending_attack_aims: AttackAims,
    pub(super) arena_rect: Option<Rect>,
    pub(super) effects_rect: Option<Rect>,
    /// The top-band HUD rects the overlay popups keep clear of, written with
    /// `effects_rect` (presentation only; CF-1-09).
    pub(super) effects_keep_clear: [Rect; 4],
    /// The worker observer window's fitted field, written each frame the
    /// window draws a live attempt and cleared when it closes (presentation
    /// only; review captures read it).
    pub(super) observer_arena_rect: Option<Rect>,
    /// The lanes the construction panel hides a live enemy in, in lane
    /// order: written each frame construction is open, cleared when it closes
    /// (presentation only; the INCOMING chevrons mark them).
    pub(super) occluded_lanes: Vec<u8>,
    /// The rows the control bar took in the last drawn battle frame
    /// (`battle::BarRows`); 0 before any battle frame
    /// (presentation only; review captures report it as `hud_rows`).
    pub(super) bar_rows: u8,
    /// The rect around every widget of that bar, and the key legend's drawn
    /// rect under it (presentation only; the legend stays clear of the bar,
    /// CF-1-08). `None` before any battle frame.
    pub(super) bar_bounds: Option<Rect>,
    pub(super) legend_rect: Option<Rect>,
    /// The wall-clock seconds the bar's retreat button has been held, `None`
    /// while it is not (presentation only; `battle::retreat`, UX-13).
    pub(super) retreat_hold: Option<f64>,
    /// This press of the retreat button already withdrew or was cancelled by
    /// Escape: it holds nothing more until the button is released.
    pub(super) retreat_hold_spent: bool,
    /// The egui frame the retreat button last drew in. A hold older than the
    /// frame before this one belongs to a battle that ended under it.
    pub(super) retreat_hold_frame: u64,
    /// The main battle's boss banner: which warning or arrival it announces
    /// and its wall-clock seconds shown (presentation only;
    /// `battle::boss_banner`, UX-11). `None` before the first.
    pub(super) boss_banner: Option<super::battle::BossBanner>,
    /// The same for the worker observer window, keyed by the observed
    /// expedition's battle id.
    pub(super) observer_boss_banner: Option<super::battle::BossBanner>,
    /// Which report the Results screen last sounded, and its loot reveal and
    /// coin batch still due (presentation only; `notices::results_audio`,
    /// A6-14). It is keyed by attempt id, and attempt ids start over in a new
    /// journey, so a new or reloaded journey clears it.
    pub(super) results_audio: horde_audio::loot::ResultsAudioGuard,
    /// Where the main battle painted its boss banner in the last drawn
    /// battle frame, `None` when it painted none (presentation only).
    pub(super) boss_banner_rect: Option<Rect>,
    /// Where the main battle painted its READY / PAUSED banner in the last
    /// drawn battle frame, `None` when it painted none (presentation only;
    /// CF-1-01).
    pub(super) pause_banner_rect: Option<Rect>,
    /// Whether the main battle's defender is low on HP, with the 30/35%
    /// hysteresis of `battle::low_hp` (presentation only; UX-12): the edge
    /// band and the HUD's "· LOW" read it. Written each battle frame.
    pub(super) low_hp: bool,
    /// The attempt `low_hp` was last judged for; another attempt starts from
    /// "not low".
    pub(super) low_hp_attempt: u64,
    pub(super) arena_hover: bool,
    pub(super) last_control_frame: u64,
    pub(super) last_toggle_frame: u64,
    /// Pre-tick actor positions and this frame's blend; see `battle_view`.
    pub(crate) interpolation: crate::battle_view::ActorInterpolation,
    /// The storm lightning clock in ms: real time over unpaused frames, or
    /// the attempt's tick in a fixed-step export (presentation only; see
    /// `battle_view::interpolation`).
    pub(crate) sky_wall_ms: f64,
}
impl BattleUi {
    /// This frame's lightning clock at the chosen observation speed.
    pub(crate) fn sky_clock(&self) -> crate::art::SkyClock {
        crate::art::SkyClock {
            wall_ms: self.sky_wall_ms,
            speed: self.speed,
        }
    }

    fn new(aim: [f32; 2]) -> Self {
        Self {
            construction: false,
            build_kind: StructureKind::Trap,
            attack_selection: AttackSelection::Gun,
            loot_feedback: loot::Feedback::default(),
            damage_feedback: damage::Feedback::default(),
            fire_latched: false,
            aim,
            inspect_log: false,
            speed: 1,
            pending: Input::default(),
            pending_attack_aims: AttackAims::default(),
            arena_rect: None,
            effects_rect: None,
            effects_keep_clear: [Rect::NOTHING; 4],
            observer_arena_rect: None,
            occluded_lanes: Vec::new(),
            bar_rows: 0,
            bar_bounds: None,
            legend_rect: None,
            retreat_hold: None,
            retreat_hold_spent: false,
            retreat_hold_frame: u64::MAX,
            boss_banner: None,
            observer_boss_banner: None,
            results_audio: horde_audio::loot::ResultsAudioGuard::default(),
            boss_banner_rect: None,
            pause_banner_rect: None,
            low_hp: false,
            low_hp_attempt: u64::MAX,
            arena_hover: false,
            last_control_frame: u64::MAX,
            last_toggle_frame: u64::MAX,
            interpolation: crate::battle_view::ActorInterpolation::default(),
            sky_wall_ms: 0.0,
        }
    }
    /// Opens or closes the construction overlay. Closing clears
    /// `occluded_lanes` at once, whether or not a battle frame follows.
    /// Whether the retreat button is being held as of egui frame `frame`: a
    /// hold the bar drew in this frame or the one before.
    pub(super) fn retreat_holding(&self, frame: u64) -> bool {
        self.retreat_hold.is_some() && frame.wrapping_sub(self.retreat_hold_frame) <= 1
    }
    /// Escape during a hold: the hold ends, and the press that began it holds
    /// nothing more.
    pub(super) fn cancel_retreat_hold(&mut self) {
        self.retreat_hold = None;
        self.retreat_hold_spent = true;
    }
    /// A new or reloaded journey: attempt ids start over, so a banner kept
    /// for the old journey's attempt would mark the new one's warning as
    /// already announced.
    pub(super) fn clear_boss_banners(&mut self) {
        self.boss_banner = None;
        self.observer_boss_banner = None;
    }
    pub(super) fn set_construction(&mut self, open: bool) {
        self.construction = open;
        if !open {
            self.occluded_lanes.clear();
        }
    }
}

/// Settings screen drafts and key rebinding.
#[derive(Resource, Default)]
pub(crate) struct SettingsUi {
    pub(super) balance_draft: Option<horde_core::balance::BalanceSettings>,
    /// The player chose Custom over a named draft. It highlights Custom and
    /// makes Apply store an unedited named draft as Custom on that preset's
    /// basis (R21, `Campaign::custom_balance_steps`); the draft keeps the
    /// named preset so the core keeps its extras.
    pub(super) balance_custom_chosen: bool,
    /// The interface scale while its slider is held (UX-10). `Settings::scale`
    /// keeps the applied value until the drag ends, so the screen does not
    /// rescale under the pointer; `None` whenever the slider is at rest.
    pub(super) scale_draft: Option<f32>,
    /// The core Controls row waiting for a key.
    pub(super) rebind: Option<Action>,
    /// The extra Controls row waiting for a key. At most one of `rebind` and
    /// `rebind_extra` is set (`start_rebind`).
    pub(super) rebind_extra: Option<ExtraAction>,
    pub(super) binding_armed: bool,
}
impl SettingsUi {
    /// The Controls row waiting for a key, in either table.
    pub(super) fn rebinding(&self) -> Option<BindingId> {
        self.rebind
            .map(BindingId::Core)
            .or(self.rebind_extra.map(BindingId::Extra))
    }
    /// Opens the key prompt of `id`, closing any other. The prompt arms on
    /// the next frame, so the click that opened it is never captured.
    pub(super) fn start_rebind(&mut self, id: BindingId) {
        (self.rebind, self.rebind_extra) = match id {
            BindingId::Core(action) => (Some(action), None),
            BindingId::Extra(action) => (None, Some(action)),
        };
        self.binding_armed = false;
    }
    /// Closes the key prompt without binding.
    pub(super) fn cancel_rebind(&mut self) {
        self.rebind = None;
        self.rebind_extra = None;
    }
}

#[derive(Resource, Default)]
pub(crate) struct MenuUi {
    pub(super) new_journey_confirm: bool,
}

/// Every frontend resource as owned values. Startup builds one, applies review
/// or practice scenarios, then `insert_into` splits it into App resources and
/// states. Tests drive widgets against one directly through `ctx()`.
pub(crate) struct FrontendParts {
    pub(super) game: Game,
    pub(super) runtime: Runtime,
    pub(super) settings: Settings,
    pub(super) paths: ProfilePaths,
    pub(super) launch: LaunchConfig,
    pub(super) textures: ArtTextures,
    pub(super) screen: Screen,
    pub(super) next_screen: NextState<Screen>,
    pub(super) next_tab: NextState<Tab>,
    pub(super) nav: Navigation,
    pub(super) status: StatusUi,
    pub(super) campaign: CampaignUi,
    pub(super) armory: ArmoryUi,
    pub(super) battle: BattleUi,
    pub(super) history: HistoryUi,
    pub(super) prefs: SettingsUi,
    pub(super) menu: MenuUi,
    pub(super) cues: Vec<SoundCue>,
    pub(super) notices: Vec<RuntimeNotice>,
    /// Stands in for `RenderPlan::pipelined`, which the App's widget pass
    /// reads from the display plugin's resource.
    pub(super) plan_pipelined: bool,
}
impl FrontendParts {
    /// Production startup: reads the save beside the one startup settings
    /// read `main` made before the App (never `settings.json` again), then
    /// applies review or practice scenarios.
    pub fn load(config: &LaunchConfig, startup_settings: SettingsLoad) -> Self {
        let paths = ProfilePaths::from_config(config);
        // The read ran before logging existed; report its warning now.
        if let Some(warning) = &startup_settings.warning {
            warn!("{warning}");
        }
        let profile = LoadedProfile::load(&paths, startup_settings);
        let resumed_practice = config.practice
            && (profile.game.campaign().selected_survivor.is_some()
                || profile.game.campaign().attempts > 0
                || profile.game.campaign().world.start.is_some());
        let mut f = Self::new(profile, config, &paths);
        if !resumed_practice {
            super::review::prepare(&mut f);
        } else {
            f.status.message =
                "Practice scenario - saved progress is isolated from your campaign".into();
        }
        super::review::apply_reduced_override(
            &mut f,
            config.scenario.is_some(),
            config.practice,
            config.review().is_some_and(|r| r.reduced),
        );
        super::review::apply_motion_override(&mut f, config);
        f
    }
    /// Pure construction: touches no files and no environment variables.
    pub fn new(profile: LoadedProfile, config: &LaunchConfig, paths: &ProfilePaths) -> Self {
        let profile_exists = profile.game.campaign().selected_survivor.is_some()
            || profile.game.campaign().attempts > 0
            || profile.game.campaign().world.start.is_some();
        let LoadedProfile {
            game,
            settings,
            warning: message,
            load_error,
            settings_warning,
            settings_at_risk,
        } = profile;
        // A startup storage failure opens the storage modal (UX-08): the
        // load failure first, with the settings warning behind it.
        // A settings file from before the onboarding callouts takes every
        // callout the loaded journey already unlocked (ruling S4). A save
        // that could not be read is not that journey: seeding then waits
        // for "Retry load" or a New game.
        let mut settings = settings;
        if load_error.is_none() {
            super::onboarding::seed_seen_callouts(&mut settings, &game);
        }
        let modal = super::modal::startup_modal(load_error, settings_warning);
        let mut runtime = Runtime::default();
        if settings_at_risk {
            runtime.guard_settings_original();
        }
        let selected_country = game
            .campaign()
            .world
            .active
            .clone()
            .unwrap_or_else(|| "DEU".into());
        let initial_aim = [
            game.attempt().map_or_else(
                || game.campaign().arena_geometry().1 * 0.5,
                |a| a.width_yards * 0.5,
            ),
            game.attempt().map_or_else(
                || f32::from(game.campaign().lane_count()) * 0.5,
                |a| a.defender_position()[1],
            ),
        ];
        let preview_survivor = game
            .campaign()
            .selected_survivor
            .unwrap_or(SurvivorId::Rook);
        Self {
            game,
            runtime,
            settings,
            paths: paths.clone(),
            launch: config.clone(),
            textures: ArtTextures::default(),
            screen: Screen::Menu,
            next_screen: NextState::Unchanged,
            next_tab: NextState::Unchanged,
            nav: Navigation::default(),
            status: StatusUi {
                message,
                // An isolated review stages a throwaway profile, not a practice save.
                practice_profile: config.practice,
                profile_exists,
                modal,
                ..StatusUi::default()
            },
            campaign: CampaignUi {
                mode: Mode::Personal,
                stage: 0,
                selected_country,
                preview_survivor,
                search: String::new(),
                map: MapCache::default(),
                improvement_focus: None,
                tree_search_seed: None,
                observed: None,
            },
            armory: ArmoryUi::default(),
            battle: BattleUi::new(initial_aim),
            history: HistoryUi::default(),
            prefs: SettingsUi::default(),
            menu: MenuUi::default(),
            cues: Vec::new(),
            notices: Vec::new(),
            plan_pipelined: crate::render_plan::PIPELINED_DEFAULT,
        }
    }
    /// Lends every part to widget code, as the draw system does with resources.
    pub fn ctx(&mut self) -> UiCtx<'_> {
        UiCtx {
            game: &self.game,
            runtime: &mut self.runtime,
            settings: &mut self.settings,
            paths: &self.paths,
            launch: &self.launch,
            textures: self.textures,
            screen: self.screen,
            tab: self.nav.tab,
            next_screen: &mut self.next_screen,
            next_tab: &mut self.next_tab,
            nav: &mut self.nav,
            status: &mut self.status,
            campaign: &mut self.campaign,
            armory: &mut self.armory,
            battle: &mut self.battle,
            history: &mut self.history,
            prefs: &mut self.prefs,
            menu: &mut self.menu,
            cues: &mut self.cues,
            perf: None,
            plan_pipelined: self.plan_pipelined,
        }
    }
    /// Lends the runner's view of the parts; notices collect in `self.notices`.
    #[cfg(test)]
    pub fn runtime_ctx(&mut self) -> crate::runner::RuntimeCtx<'_> {
        crate::runner::RuntimeCtx {
            game: &mut self.game,
            runtime: &mut self.runtime,
            settings: &self.settings,
            paths: &self.paths,
            notices: &mut self.notices,
            cues: &mut self.cues,
            game_changed: false,
            perf: None,
        }
    }
    /// Startup only: everything becomes resources, `screen` the initial state.
    pub fn insert_into(self, app: &mut App) {
        let Self {
            game,
            runtime,
            settings,
            paths,
            launch: _,
            textures,
            screen,
            next_screen: _,
            next_tab: _,
            nav,
            status,
            campaign,
            armory,
            battle,
            history,
            prefs,
            menu,
            cues: _,
            notices: _,
            plan_pipelined: _,
        } = self;
        // A pending sub-state value is used when the camp first exists.
        let tab = nav.tab;
        app.insert_resource(GameSession { game })
            .insert_resource(runtime)
            .insert_resource(settings)
            .insert_resource(paths)
            .insert_resource(textures)
            .insert_resource(nav)
            .insert_resource(status)
            .insert_resource(campaign)
            .insert_resource(armory)
            .insert_resource(battle)
            .insert_resource(history)
            .insert_resource(prefs)
            .insert_resource(menu)
            .insert_state(screen)
            .insert_resource(NextState::Pending(tab))
            .add_sub_state::<Tab>();
    }

    // The App runs these as schedules; startup scenarios and tests call them
    // directly on the owned parts.

    /// Runs one UI action, then the notice system and state transition.
    pub(super) fn act<R>(&mut self, action: impl FnOnce(&mut UiCtx) -> R) -> R {
        let result = action(&mut self.ctx());
        self.apply_notices();
        self.apply_transitions();
        result
    }
    /// The notice system: every collected runtime notice, in order.
    pub(super) fn apply_notices(&mut self) {
        let notices = std::mem::take(&mut self.notices);
        let mut f = self.ctx();
        for notice in notices {
            f.apply_notice(notice);
        }
    }
    /// The state transition: pending screen (with `OnEnter(Battle)`), then the
    /// camp tab sub-state, which only applies while the camp is shown.
    pub(super) fn apply_transitions(&mut self) {
        let entered = match std::mem::take(&mut self.next_screen) {
            NextState::Pending(screen) => Some(screen),
            NextState::PendingIfDifferent(screen) if screen != self.screen => Some(screen),
            _ => None,
        };
        if let Some(screen) = entered {
            self.screen = screen;
            if screen == Screen::Battle {
                notices::reset_new_battle(
                    &mut self.nav,
                    &mut self.battle,
                    &mut self.campaign,
                    &self.game,
                    &mut self.runtime,
                );
            }
        }
        if let NextState::Pending(tab) | NextState::PendingIfDifferent(tab) =
            std::mem::take(&mut self.next_tab)
            && self.screen == Screen::Camp
        {
            self.nav.tab = tab;
        }
    }
}
