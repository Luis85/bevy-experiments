//! Frontend resources, one per concern, and `FrontendParts`: the owned set of
//! them assembled at startup (and by tests) before being inserted into the App.
use super::{
    Screen, Tab, UiCtx, armory::ArmorySort, battle::AttackSelection, damage,
    feedback::FeedbackState, history::HistoryUi, loot, notices,
};
use crate::{
    audio::Cue as SoundCue,
    launch::LaunchConfig,
    profile::{LoadedProfile, ProfilePaths},
    runner::{AttackAims, GameSession, Runtime, RuntimeNotice},
    settings::{Action, Settings},
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
}
impl Default for Navigation {
    fn default() -> Self {
        Self {
            tab: Tab::World,
            last_prepare_tab: Tab::Briefing,
            history_return: Screen::Menu,
            history_world_paused: true,
            return_screen: Screen::Menu,
            return_world_paused: false,
            battle_reset: false,
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
    pub(super) arena_hover: bool,
    pub(super) custom_cursor_requested: bool,
    pub(super) last_control_frame: u64,
    pub(super) last_toggle_frame: u64,
    /// Pre-tick actor positions and this frame's blend; see `battle_view`.
    pub(crate) interpolation: crate::battle_view::ActorInterpolation,
}
impl BattleUi {
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
            arena_hover: false,
            custom_cursor_requested: false,
            last_control_frame: u64::MAX,
            last_toggle_frame: u64::MAX,
            interpolation: crate::battle_view::ActorInterpolation::default(),
        }
    }
}

/// Settings screen drafts and key rebinding.
#[derive(Resource, Default)]
pub(crate) struct SettingsUi {
    pub(super) balance_draft: Option<horde_core::balance::BalanceSettings>,
    pub(super) rebind: Option<Action>,
    pub(super) binding_armed: bool,
}

#[derive(Resource, Default)]
pub(crate) struct MenuUi {
    pub(super) new_journey_confirm: bool,
}

/// Whether the arena owns the pointer this frame (read by the native cursor).
#[derive(Resource, Default)]
pub struct CustomCursor(pub bool);

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
}
impl FrontendParts {
    /// Production startup: reads the save, then applies review or practice scenarios.
    pub fn load(config: &LaunchConfig) -> Self {
        let paths = ProfilePaths::from_config(config);
        let profile = LoadedProfile::load(&paths);
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
        // Explicit automated-motion fixtures use a separate temporary profile.
        // This changes startup state only; fixed systems still own every tick.
        if config.scenario.is_some() && !config.practice && config.motion_review() {
            f.game.set_paused(false);
            f.battle.construction = false;
            f.status.message =
                "Live motion review · progress is isolated from your campaign".into();
        }
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
            ..
        } = profile;
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
            runtime: Runtime::default(),
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
                practice_profile: config.practice || config.scenario.is_some(),
                profile_exists,
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
                observed: None,
            },
            armory: ArmoryUi::default(),
            battle: BattleUi::new(initial_aim),
            history: HistoryUi::default(),
            prefs: SettingsUi::default(),
            menu: MenuUi::default(),
            cues: Vec::new(),
            notices: Vec::new(),
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
            .init_resource::<CustomCursor>()
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
