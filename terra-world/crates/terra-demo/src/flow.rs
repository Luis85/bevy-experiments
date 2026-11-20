//! Native session lifecycle. Overlay pauses are presentation state, never saved
//! into the authoritative clock; multiplayer menus only suppress local input.
use crate::DemoOptions;
use bevy::{
    app::AppExit,
    input::InputSystems,
    input_focus::{
        AutoFocus, InputFocus,
        tab_navigation::{TabGroup, TabIndex, TabNavigationPlugin},
    },
    picking::hover::Hovered,
    prelude::*,
    ui::Pressed,
    ui_widgets::{
        Activate, Button, ControlOrientation, ScrollArea, ScrollIntoView, Scrollbar,
        ScrollbarThumb, observe,
    },
};
use std::{
    collections::VecDeque,
    path::PathBuf,
    sync::mpsc::{Receiver, SyncSender, sync_channel},
};
use terra_bevy::{
    LivingDiagnostics, NetworkStatus, RegionalPersistence, ResidentChunks, SandboxIntent,
    SandboxQueue, SandboxRuntime, WorldRuntime, WorldSet,
    render::{RenderQualitySettings, RenderTelemetry},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Page {
    Session,
    Settings,
    Controls,
    RestoreReview,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Destination {
    Stay,
    Menu,
    Quit,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Loading,
    Playing,
    Paused(Page),
    Saving {
        destination: Destination,
        generation: u64,
    },
}

#[derive(Resource)]
pub struct GameFlow {
    phase: Phase,
    dirty: bool,
    notice: Option<String>,
    return_to_menu: bool,
    virtual_was_paused: bool,
    owns_pause: bool,
    focus: Option<Action>,
    release_input_next_frame: bool,
    restore_requested: bool,
    restore_serial: u64,
    restore_review: Option<terra_bevy::ReviewedRestore>,
}
impl Default for GameFlow {
    fn default() -> Self {
        Self {
            phase: Phase::Loading,
            dirty: true,
            notice: None,
            return_to_menu: false,
            virtual_was_paused: false,
            owns_pause: false,
            focus: None,
            release_input_next_frame: false,
            restore_requested: false,
            restore_serial: 0,
            restore_review: None,
        }
    }
}
impl GameFlow {
    pub(crate) fn diagnostic_phase(&self) -> &'static str {
        match self.phase {
            Phase::Loading => "loading",
            Phase::Playing => "playing",
            Phase::Paused(_) => "paused",
            Phase::Saving { .. } => "saving",
        }
    }
    pub(crate) fn controller_menu_active(&self) -> bool {
        matches!(self.phase, Phase::Loading | Phase::Paused(_))
    }
    pub fn blocks_input(&self) -> bool {
        self.phase != Phase::Playing || self.release_input_next_frame
    }
    pub(crate) fn request_restore(&mut self) {
        self.restore_requested = true;
        self.change(Phase::Paused(Page::Session));
    }
    pub fn returns_to_menu(&self) -> bool {
        self.return_to_menu
    }
    fn change(&mut self, phase: Phase) {
        self.phase = phase;
        self.release_input_next_frame = phase == Phase::Playing;
        self.dirty = true;
        self.focus = None;
    }
}

/// App::run transfers the real world into its runner and leaves an empty App.
/// Keep the final presentation handoff outside that consumed world. This single
/// bounded record contains no simulation state or private network identity.
#[derive(Resource)]
pub(crate) struct ExitHandoff(SyncSender<ExitSnapshot>);

pub(crate) struct ExitSnapshot {
    pub return_to_menu: bool,
    pub user_data: PathBuf,
    pub menu_supervised: bool,
    pub profile: Option<PathBuf>,
    pub settings: RenderQualitySettings,
    pub controls: crate::player_input::InputSettings,
    pub config: terra_bevy::WorldConfig,
}

pub(crate) fn exit_handoff() -> (ExitHandoff, Receiver<ExitSnapshot>) {
    let (sender, receiver) = sync_channel(1);
    (ExitHandoff(sender), receiver)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Action {
    Resume,
    Session,
    Settings,
    Controls,
    Checkpoint,
    Restore,
    ConfirmRestore(u64),
    Menu,
    Quit,
    CloseFailedWorld,
    Quality,
    Sky,
    DayNight,
    Seasons,
    TimeScale,
    SaveSettings,
    InputSetting(crate::player_input::SettingAction),
}
#[derive(Resource, Default)]
struct Actions(VecDeque<Action>);
#[derive(Component)]
struct Overlay;
#[derive(Component)]
struct StatusText;
#[derive(Component)]
struct FlowButton;

const TERMINAL_STORAGE_NOTICE: &str = "The storage worker stopped. Live progress is unconfirmed; a submitted checkpoint may have committed. Close this world and reopen it to validate saved progress. Save and Restore cannot recover this session.";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TerminalStorageAction {
    Normal,
    RemainOpen,
    Close,
}

/// Presentation policy over the worker's observed failure classification. This
/// cannot change that classification or replace/restart a storage worker.
fn terminal_storage_action(
    action: Action,
    network_authority: bool,
    requires_reopen: bool,
    flow: &mut GameFlow,
) -> TerminalStorageAction {
    let terminal = !network_authority && requires_reopen;
    if action == Action::CloseFailedWorld {
        if terminal && matches!(flow.phase, Phase::Loading | Phase::Paused(Page::Session)) {
            flow.return_to_menu = true;
            flow.notice = Some(TERMINAL_STORAGE_NOTICE.into());
            return TerminalStorageAction::Close;
        }
        flow.notice = Some("Use the normal save and exit controls for this session.".into());
        return TerminalStorageAction::RemainOpen;
    }
    if terminal
        && matches!(
            action,
            Action::Resume
                | Action::Checkpoint
                | Action::Restore
                | Action::ConfirmRestore(_)
                | Action::Menu
                | Action::Quit
        )
    {
        flow.notice = Some(TERMINAL_STORAGE_NOTICE.into());
        return TerminalStorageAction::RemainOpen;
    }
    TerminalStorageAction::Normal
}

fn session_buttons(
    phase: Phase,
    network_authority: bool,
    requires_reopen: bool,
) -> Vec<(&'static str, Action)> {
    let terminal = !network_authority && requires_reopen;
    match phase {
        Phase::Loading if terminal => vec![("Close failed world", Action::CloseFailedWorld)],
        Phase::Loading => vec![(
            if network_authority {
                "Cancel connection · Main menu"
            } else {
                "Save and return to main menu"
            },
            Action::Menu,
        )],
        Phase::Paused(Page::Session) if terminal => vec![
            ("Close failed world", Action::CloseFailedWorld),
            ("Settings", Action::Settings),
            ("Controls", Action::Controls),
        ],
        Phase::Paused(Page::Session) => {
            let mut buttons = vec![("Resume world · Escape", Action::Resume)];
            if !network_authority {
                buttons.push(("Checkpoint now · F5", Action::Checkpoint));
            }
            buttons.extend([
                ("Settings", Action::Settings),
                ("Controls", Action::Controls),
            ]);
            if !network_authority {
                buttons.push((
                    "Restore latest checkpoint · F9 · Discards later changes",
                    Action::Restore,
                ));
            }
            buttons.extend([
                (
                    if network_authority {
                        "Leave server · Main menu"
                    } else {
                        "Save and return to main menu"
                    },
                    Action::Menu,
                ),
                (
                    if network_authority {
                        "Leave server and quit"
                    } else {
                        "Save and quit"
                    },
                    Action::Quit,
                ),
            ]);
            buttons
        }
        _ => Vec::new(),
    }
}

pub struct GameFlowPlugin;
impl Plugin for GameFlowPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GameFlow>().init_resource::<Actions>();
        if !app.is_plugin_added::<TabNavigationPlugin>() {
            app.add_plugins(TabNavigationPlugin);
        }
        app.add_systems(
            PreUpdate,
            keyboard_and_pause
                .after(InputSystems)
                .after(crate::command_input::CommandCapture)
                .after(crate::player_input::PlayerInputSet),
        )
        .add_systems(
            Update,
            process_actions
                .before(WorldSet::Sandbox)
                .after(crate::accessibility::ControllerNavigation),
        )
        .add_systems(
            Update,
            (poll_state, rebuild, update_status, style)
                .chain()
                .after(WorldSet::Render),
        )
        .add_systems(
            Last,
            save_managed_json_on_exit
                .in_set(bevy::app::OnAppExitSystems)
                .in_set(terra_bevy::RegionPersistenceSet::FinalCheckpoint)
                .after(bevy::window::ExitSystems),
        )
        .add_systems(
            Last,
            capture_exit_handoff
                .in_set(bevy::app::OnAppExitSystems)
                .after(bevy::window::ExitSystems)
                .after(terra_bevy::RegionPersistenceSet::FinalCheckpoint),
        )
        .add_systems(
            PostUpdate,
            scroll_focus_into_view.after(bevy::ui::UiSystems::Layout),
        );
    }
}

fn capture_exit_handoff(
    mut exits: ParamSet<(MessageReader<AppExit>, MessageWriter<AppExit>)>,
    handoff: Res<ExitHandoff>,
    options: Res<DemoOptions>,
    flow: Res<GameFlow>,
    settings: Res<RenderQualitySettings>,
    controls: Res<crate::player_input::InputSettings>,
    runtime: Res<WorldRuntime>,
    mut captured: Local<bool>,
) {
    if *captured || exits.p0().read().next().is_none() {
        return;
    }
    *captured = true;
    if let Err(error) = handoff.0.try_send(ExitSnapshot {
        return_to_menu: flow.returns_to_menu(),
        user_data: options.user_data.clone(),
        menu_supervised: options.menu_supervised,
        profile: options.world_profile_path.clone(),
        settings: *settings,
        controls: controls.clone(),
        config: runtime.config.clone(),
    }) {
        eprintln!("Terra World: final session handoff could not be retained: {error}");
        exits.p1().write(AppExit::error());
    }
}

fn keyboard_and_pause(
    keys: Res<ButtonInput<KeyCode>>,
    runtime: Res<WorldRuntime>,
    mut flow: ResMut<GameFlow>,
    mut virtual_time: ResMut<Time<Virtual>>,
    mut actions: ResMut<Actions>,
    mut ui: Option<ResMut<crate::interface::UiState>>,
    mut placement: Option<ResMut<crate::interaction::Placement>>,
    mut politics: ResMut<terra_bevy::PoliticalWorld>,
    mut sandbox: ResMut<SandboxRuntime>,
    input: Res<crate::player_input::PlayerInput>,
    keyboard: Option<Res<crate::accessibility::ControllerKeyboard>>,
) {
    // A menu activation key must not also drive, jump or toggle the world clock
    // during the Update in which Resume closes the overlay.
    flow.release_input_next_frame = false;
    if (keys.just_pressed(KeyCode::Escape)
        || input.menu_pressed
        || (input.menu_back && flow.controller_menu_active()))
        && !input.suppress
        && !input.command_consumed
        && keyboard.as_ref().is_none_or(|keyboard| !keyboard.is_open())
    {
        match flow.phase {
            Phase::Playing => {
                // Cancel the most immediate edit first. Consume the closing
                // frame so this Escape cannot also pause or reach an in-world
                // action after its editing/placement gate has been cleared.
                let cancelled = if let Some(ui) = ui.as_deref_mut().filter(|ui| ui.editing_gps) {
                    ui.editing_gps = false;
                    Some("Text entry cancelled.")
                } else if let Some(placement) = placement
                    .as_deref_mut()
                    .filter(|placement| placement.is_active())
                {
                    placement.0 = None;
                    Some("Placement cancelled.")
                } else if politics.drawing || !politics.draft.is_empty() {
                    politics.drawing = false;
                    politics.draft.clear();
                    Some("Territory drawing cancelled.")
                } else {
                    None
                };
                if let Some(message) = cancelled {
                    sandbox.last_message = message.into();
                    flow.release_input_next_frame = true;
                } else {
                    flow.notice = None;
                    flow.change(Phase::Paused(Page::Session));
                }
            }
            Phase::Paused(Page::Session) => actions.0.push_back(Action::Resume),
            Phase::Paused(_) => actions.0.push_back(Action::Session),
            _ => {}
        }
    }
    if matches!(flow.phase, Phase::Paused(_))
        && flow.phase != Phase::Paused(Page::RestoreReview)
        && !input.suppress
        && keyboard.as_ref().is_none_or(|keyboard| !keyboard.is_open())
    {
        if keys.just_pressed(KeyCode::F5) {
            actions.0.push_back(Action::Checkpoint);
        }
        if keys.just_pressed(KeyCode::F9) {
            actions.0.push_back(Action::Restore);
        }
    }
    // The local virtual time is independent of the saved WorldClock. Loading
    // and pause overlays therefore never turn a running save into a paused one.
    if flow.blocks_input() && !runtime.network_authority {
        if !flow.owns_pause {
            flow.virtual_was_paused = virtual_time.is_paused();
            flow.owns_pause = true;
        }
        virtual_time.pause();
    } else if flow.owns_pause {
        if !flow.virtual_was_paused {
            virtual_time.unpause();
        }
        flow.owns_pause = false;
    }
}

fn process_actions(
    mut flow: ResMut<GameFlow>,
    mut actions: ResMut<Actions>,
    mut runtime: ResMut<WorldRuntime>,
    mut quality: ResMut<RenderQualitySettings>,
    mut sandbox: ResMut<SandboxRuntime>,
    mut queue: ResMut<SandboxQueue>,
    persistence: Res<RegionalPersistence>,
    network: Option<Res<NetworkStatus>>,
    options: Res<DemoOptions>,
    mut exit: MessageWriter<AppExit>,
    mut input_settings: ResMut<crate::player_input::InputSettings>,
) {
    if flow.restore_requested {
        flow.restore_requested = false;
        actions.0.push_front(Action::Restore);
    }
    if input_settings.is_changed() && matches!(flow.phase, Phase::Paused(Page::Controls)) {
        flow.dirty = true;
    }
    if !matches!(flow.phase, Phase::Paused(Page::Controls)) {
        input_settings.cancel_capture();
    }
    for _ in 0..16 {
        let Some(action) = actions.0.pop_front() else {
            break;
        };
        if matches!(flow.phase, Phase::Saving { .. }) {
            continue;
        }
        if flow.phase == Phase::Paused(Page::RestoreReview)
            && !matches!(action, Action::ConfirmRestore(_) | Action::Session)
        {
            continue;
        }
        if let Action::ConfirmRestore(serial) = action {
            if flow.phase != Phase::Paused(Page::RestoreReview) || serial != flow.restore_serial {
                continue;
            }
        }
        flow.focus = Some(action);
        match terminal_storage_action(
            action,
            runtime.network_authority,
            persistence.requires_reopen(),
            &mut flow,
        ) {
            TerminalStorageAction::Close => {
                actions.0.clear();
                eprintln!(
                    "Terra World: closing failed storage session; live progress is unconfirmed. Reopen the world to validate committed progress."
                );
                exit.write(AppExit::error());
                break;
            }
            TerminalStorageAction::RemainOpen => {
                flow.dirty = true;
                continue;
            }
            TerminalStorageAction::Normal => {}
        }
        match action {
            Action::InputSetting(action) => input_settings.handle(action, &options.user_data),
            Action::Resume => {
                if runtime.network_authority
                    && network.as_ref().is_none_or(|status| !status.connected)
                {
                    flow.notice = Some("The server connection has ended. Return to the main menu and reconnect with the same session name to resume your identity.".into());
                } else if !persistence.blocked() {
                    flow.notice = None;
                    flow.change(Phase::Playing);
                } else {
                    flow.notice = Some("Progress is blocked by a storage error. Retry the checkpoint or restore the last successful one before resuming.".into());
                }
            }
            Action::Session => {
                flow.restore_review = None;
                flow.change(Phase::Paused(Page::Session));
            }
            Action::Settings => flow.change(Phase::Paused(Page::Settings)),
            Action::Controls => flow.change(Phase::Paused(Page::Controls)),
            Action::Checkpoint | Action::Menu | Action::Quit => {
                let destination = match action {
                    Action::Menu => Destination::Menu,
                    Action::Quit => Destination::Quit,
                    _ => Destination::Stay,
                };
                if runtime.network_authority {
                    if destination == Destination::Stay {
                        flow.notice = Some(
                            "Your host owns checkpoints. The client cannot save the shared world."
                                .into(),
                        );
                    } else {
                        flow.return_to_menu = destination == Destination::Menu;
                        exit.write(AppExit::Success);
                    }
                } else {
                    let generation = persistence
                        .status
                        .as_ref()
                        .map(|s| s.generation)
                        .unwrap_or(0);
                    sandbox.last_message = "Waiting for the requested checkpoint…".into();
                    queue.0.push_back(SandboxIntent::Save);
                    flow.notice = None;
                    flow.change(Phase::Saving {
                        destination,
                        generation,
                    });
                }
            }
            Action::Restore => {
                if !queue.0.is_empty() {
                    flow.notice =
                        Some("Wait for queued world actions before reviewing restore.".into());
                } else {
                    match terra_bevy::ReviewedRestore::capture(&runtime, &sandbox, &persistence) {
                        Ok(review) => {
                            let Some(serial) = flow.restore_serial.checked_add(1) else {
                                flow.notice = Some(
                                    "Restore review identity exhausted. Reopen the world.".into(),
                                );
                                continue;
                            };
                            flow.restore_serial = serial;
                            flow.restore_review = Some(review);
                            flow.notice = None;
                            flow.change(Phase::Paused(Page::RestoreReview));
                        }
                        Err(error) => flow.notice = Some(error),
                    }
                }
            }
            Action::ConfirmRestore(_) => {
                if flow.phase != Phase::Paused(Page::RestoreReview) {
                    continue;
                }
                if let Some(review) = flow.restore_review.take() {
                    if persistence.pending() || !queue.0.is_empty() {
                        flow.notice = Some(
                            "Checkpoint or queued actions changed. Review restore again.".into(),
                        );
                    } else {
                        queue.0.push_back(SandboxIntent::LoadReviewed(review));
                        flow.notice = Some("Reviewed restore requested. Later unsaved changes will be discarded only if the reviewed target still matches.".into());
                    }
                }
                flow.change(Phase::Paused(Page::Session));
            }
            Action::Quality => quality.quality = quality.quality.next(),
            Action::Sky => quality.atmospheric_sky = !quality.atmospheric_sky,
            Action::DayNight if !runtime.network_authority => {
                runtime.config.day_night = !runtime.config.day_night
            }
            Action::Seasons if !runtime.network_authority => {
                runtime.config.seasons = !runtime.config.seasons
            }
            Action::TimeScale if !runtime.network_authority => {
                runtime.clock.time_scale = match runtime.clock.time_scale as u32 {
                    1 => 15.,
                    15 => 60.,
                    60 => 120.,
                    _ => 1.,
                };
                runtime.config.time_scale = runtime.clock.time_scale;
            }
            Action::SaveSettings => {
                let result = if let Some(profile) = &options.world_profile_path {
                    crate::launcher::model::persist_settings(profile, *quality, &runtime.config)
                } else {
                    crate::launcher::model::Preferences::save_quality(&options.user_data, *quality)
                };
                flow.notice = Some(match result { Ok(()) => "Settings saved. Graphics update in this session; world time is preserved by the next checkpoint.".into(), Err(error) => format!("Settings could not be saved: {error}") });
            }
            _ => {}
        }
        if !matches!(flow.phase, Phase::Paused(Page::Controls)) {
            input_settings.cancel_capture();
        }
        flow.dirty = true;
    }
}

fn checkpoint_finished(
    generation: u64,
    persistence: &RegionalPersistence,
    queue: &SandboxQueue,
    message: &str,
) -> Option<Result<(), String>> {
    if queue
        .0
        .iter()
        .any(|intent| matches!(intent, SandboxIntent::Save))
        || persistence.pending()
    {
        return None;
    }
    if persistence.enabled {
        if persistence.blocked() {
            return Some(Err(persistence
                .last_error
                .clone()
                .unwrap_or_else(|| "The checkpoint failed.".into())));
        }
        return (persistence
            .status
            .as_ref()
            .is_some_and(|s| s.generation > generation))
        .then_some(Ok(()));
    }
    if message.starts_with("Saved world, factions, politics and clock to ") {
        Some(Ok(()))
    } else if message.starts_with("Action rejected:") {
        Some(Err(message.to_owned()))
    } else {
        None
    }
}

#[cfg(test)]
#[derive(Resource)]
struct ExitPublicationSync(fn(&std::path::Path) -> std::io::Result<()>);

fn save_managed_json_on_exit(
    mut exits: ParamSet<(MessageReader<AppExit>, MessageWriter<AppExit>)>,
    options: Res<DemoOptions>,
    runtime: Res<WorldRuntime>,
    mut sandbox: ResMut<SandboxRuntime>,
    politics: Res<terra_bevy::PoliticalWorld>,
    mut done: Local<bool>,
    #[cfg(test)] publication_sync: Option<Res<ExitPublicationSync>>,
) {
    if exits.p0().read().next().is_none() || *done {
        return;
    }
    *done = true;
    if options.world_profile_path.is_none()
        || runtime.network_authority
        || runtime.config.region_store.is_some()
    {
        return;
    }
    let save = terra_bevy::sandbox::LocalWorldSave {
        schema_version: 1,
        identity: terra_bevy::sandbox::WorldIdentity::from(&runtime.config),
        clock: runtime.clock,
        player_id: sandbox.player_id,
        state: sandbox.state.clone(),
        politics: politics.dataset.clone(),
    };
    let sync: fn(&std::path::Path) -> std::io::Result<()> =
        terra_bevy::rules::publication::sync_directory;
    #[cfg(test)]
    let sync = publication_sync
        .as_ref()
        .map_or(sync, |boundary| boundary.0);
    if let Err(error) = save
        .validate_for(&runtime.config, &sandbox.catalog)
        .and_then(|_| save.write_with_sync(&runtime.config.save_path, sync))
    {
        eprintln!("Terra World: final portable save failed: {error}");
        exits.p1().write(AppExit::error());
    } else {
        sandbox.dirty = false;
    }
}

fn poll_state(
    mut flow: ResMut<GameFlow>,
    runtime: Res<WorldRuntime>,
    resident: Res<ResidentChunks>,
    render: Res<RenderTelemetry>,
    sandbox: Res<SandboxRuntime>,
    queue: Res<SandboxQueue>,
    persistence: Res<RegionalPersistence>,
    network: Option<Res<NetworkStatus>>,
    mut exit: MessageWriter<AppExit>,
) {
    match flow.phase {
        Phase::Loading => {
            let (wanted, sourced, rendered) =
                loading_terrain_counts(&runtime, &sandbox, &resident, &render);
            let account_ready = sandbox
                .state
                .living
                .accounts
                .contains_key(&sandbox.player_id);
            let network_ready = !runtime.network_authority
                || network
                    .as_ref()
                    .is_some_and(|n| n.connected && n.actor_id.is_some());
            if wanted > 0
                && sourced >= wanted
                && rendered >= wanted
                && account_ready
                && network_ready
                && !persistence.pending()
                && !persistence.blocked()
            {
                flow.change(Phase::Playing);
            }
        }
        Phase::Saving {
            destination,
            generation,
        } => {
            if let Some(result) =
                checkpoint_finished(generation, &persistence, &queue, &sandbox.last_message)
            {
                match result {
                    Ok(()) if destination != Destination::Stay => {
                        flow.return_to_menu = destination == Destination::Menu;
                        exit.write(AppExit::Success);
                    }
                    Ok(()) => {
                        flow.change(Phase::Paused(Page::Session));
                        flow.notice =
                            Some("Checkpoint complete. Progress is safely stored.".into());
                    }
                    Err(error) => {
                        flow.change(Phase::Paused(Page::Session));
                        flow.notice = Some(format!(
                            "Checkpoint failed; the session is still open. {error}"
                        ));
                    }
                }
            }
        }
        Phase::Playing
            if runtime.network_authority
                && network.as_ref().is_none_or(|status| !status.connected) =>
        {
            flow.change(Phase::Paused(Page::Session));
            flow.notice = Some("Connection ended. Your host retains authoritative progress; return to the menu and reconnect with the same session name.".into());
        }
        _ => {}
    }
}

fn loading_position(runtime: &WorldRuntime, sandbox: &SandboxRuntime) -> terra_core::GeoPosition {
    sandbox
        .state
        .players
        .get(&sandbox.player_id)
        .map_or(runtime.focus, |player| player.position)
}

/// Use the same geographic neighborhood as population bootstrap. The renderer
/// reports current integrated source identities; unrelated pages and visible
/// fallback geometry from a replaced source cannot make this world ready.
fn loading_terrain_counts(
    runtime: &WorldRuntime,
    sandbox: &SandboxRuntime,
    resident: &ResidentChunks,
    render: &RenderTelemetry,
) -> (usize, usize, usize) {
    let keys = terra_bevy::living::startup_neighborhood(
        &runtime.config,
        loading_position(runtime, sandbox),
    );
    let wanted = keys.len().min(9);
    let sourced = keys
        .iter()
        .filter(|key| resident.chunks.contains_key(key))
        .count();
    let rendered = keys
        .iter()
        .filter(|key| resident.chunks.contains_key(key) && render.integrated_chunks.contains(key))
        .count();
    (wanted, sourced, rendered)
}

const PANEL: Color = Color::srgba(0.055, 0.095, 0.083, 0.98);
const SURFACE: Color = Color::srgb(0.105, 0.188, 0.16);
const TEXT: Color = Color::srgb(0.933, 0.945, 0.894);
const MUTED: Color = Color::srgb(0.698, 0.769, 0.710);
const ACCENT: Color = Color::srgb(0.702, 0.863, 0.557);

/// Shared native menu scrolling. Tab focus follows its control into view and a
/// real draggable thumb remains available when content overflows the panel.
pub(crate) fn scroll_panel(
    parent: &mut ChildSpawnerCommands,
    max_vh: f32,
    gap: f32,
    build: impl FnOnce(&mut ChildSpawnerCommands),
) {
    parent
        .spawn(Node {
            width: percent(100),
            max_height: vh(max_vh),
            min_height: px(0),
            column_gap: px(8),
            ..default()
        })
        .with_children(|p| {
            let target = p
                .spawn((
                    ScrollArea,
                    Node {
                        flex_grow: 1.,
                        flex_basis: px(0),
                        min_width: px(0),
                        max_height: vh(max_vh),
                        min_height: px(0),
                        overflow: Overflow::scroll_y(),
                        flex_direction: FlexDirection::Column,
                        row_gap: px(gap),
                        padding: UiRect::right(px(4)),
                        ..default()
                    },
                ))
                .with_children(build)
                .id();
            p.spawn((
                Scrollbar::new(target, ControlOrientation::Vertical, 24.),
                Node {
                    width: px(8),
                    align_self: AlignSelf::Stretch,
                    flex_shrink: 0.,
                    border_radius: BorderRadius::all(px(4)),
                    ..default()
                },
                BackgroundColor(Color::srgb(0.09, 0.15, 0.12)),
            ))
            .with_children(|p| {
                p.spawn((
                    ScrollbarThumb {
                        border_radius: BorderRadius::all(px(4)),
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.35, 0.49, 0.37)),
                ));
            });
        });
}

pub(crate) fn scroll_focus_into_view(
    mut commands: Commands,
    focus: Res<InputFocus>,
    mut previous: Local<Option<Entity>>,
    computed: Query<&ComputedNode>,
    mut bars: Query<(&Scrollbar, &mut Visibility)>,
) {
    if focus.get() != *previous {
        *previous = focus.get();
        if let Some(entity) = focus.get() {
            commands.trigger(ScrollIntoView { entity });
        }
    }
    for (bar, mut visibility) in &mut bars {
        if let Ok(node) = computed.get(bar.target) {
            *visibility = if node.content_size().y > node.size().y + 1. {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            };
        }
    }
}

fn label(value: impl Into<String>, size: f32, color: Color) -> impl Bundle {
    (
        Text::new(value),
        TextLayout::linebreak(bevy::text::LineBreak::WordOrCharacter),
        TextFont {
            font_size: FontSize::Px(size),
            ..default()
        },
        TextColor(color),
        Node {
            flex_shrink: 0.,
            max_width: percent(100),
            ..default()
        },
    )
}
fn button(
    parent: &mut ChildSpawnerCommands,
    title: impl Into<String>,
    action: Action,
    focus: Option<Action>,
) {
    let mut entity = parent.spawn((
        Button,
        FlowButton,
        TabIndex(0),
        Hovered::default(),
        Node {
            min_height: px(40),
            width: percent(100),
            padding: UiRect::axes(px(12), px(10)),
            border: UiRect::all(px(1)),
            border_radius: BorderRadius::all(px(5)),
            flex_shrink: 0.,
            ..default()
        },
        BackgroundColor(SURFACE),
        BorderColor::all(Color::NONE),
        observe(
            move |_: On<Activate>,
                  mut actions: ResMut<Actions>,
                  settings: Res<crate::player_input::InputSettings>,
                  input: Res<crate::player_input::PlayerInput>,
                  keys: Res<ButtonInput<KeyCode>>,
                  keyboard: Option<Res<crate::accessibility::ControllerKeyboard>>| {
                if keyboard
                    .as_ref()
                    .is_some_and(|keyboard| keyboard.blocks_parent_actions())
                    || crate::player_input::capture_consumes_activation(&settings, &input, &keys)
                {
                    return;
                }
                if actions.0.len() < 32 {
                    actions.0.push_back(action);
                }
            },
        ),
    ));
    if focus == Some(action) {
        entity.insert(AutoFocus);
    }
    entity.with_children(|p| {
        p.spawn(label(title, 14., TEXT));
    });
}

fn rebuild(
    mut commands: Commands,
    mut flow: ResMut<GameFlow>,
    roots: Query<Entity, With<Overlay>>,
    runtime: Res<WorldRuntime>,
    quality: Res<RenderQualitySettings>,
    options: Res<DemoOptions>,
    mut focus: ResMut<InputFocus>,
    input_settings: Res<crate::player_input::InputSettings>,
    input: Res<crate::player_input::PlayerInput>,
    persistence: Res<RegionalPersistence>,
    mut last_terminal: Local<bool>,
) {
    let terminal = !runtime.network_authority && persistence.requires_reopen();
    if !flow.dirty && *last_terminal == terminal {
        return;
    }
    *last_terminal = terminal;
    flow.dirty = false;
    focus.clear();
    for root in &roots {
        commands.entity(root).despawn();
    }
    if flow.phase == Phase::Playing {
        return;
    }
    let phase = flow.phase;
    let terminal_surface =
        terminal && matches!(phase, Phase::Loading | Phase::Paused(Page::Session));
    let choices = session_buttons(
        phase,
        runtime.network_authority,
        persistence.requires_reopen(),
    );
    let initial = flow
        .focus
        .filter(|action| !terminal_surface || choices.iter().any(|(_, choice)| choice == action))
        .or(Some(match phase {
            _ if terminal_surface => Action::CloseFailedWorld,
            Phase::Paused(Page::Settings) => Action::Quality,
            Phase::Paused(Page::Controls) | Phase::Paused(Page::RestoreReview) => Action::Session,
            Phase::Paused(_) => Action::Resume,
            _ => Action::Menu,
        }));
    commands.spawn((Overlay, TabGroup::modal(), GlobalZIndex(1000), Node { position_type: PositionType::Absolute, width: percent(100), height: percent(100), justify_content: JustifyContent::Center, align_items: AlignItems::Center, padding: UiRect::all(px(24)), ..default() }, BackgroundColor(Color::srgba(0.02, 0.055, 0.04, 0.70)))).with_children(|root| {
        root.spawn((Node { width: px(620), max_width: percent(94), max_height: percent(100), padding: UiRect::all(px(24)), flex_direction: FlexDirection::Column, row_gap: px(14), border_radius: BorderRadius::all(px(9)), border: UiRect::all(px(1)), ..default() }, BackgroundColor(PANEL), BorderColor::all(Color::srgb(0.28, 0.39, 0.31)))).with_children(|p| {
            let (title, subtitle) = match phase {
                _ if terminal_surface => ("World needs reopening", "Live progress is unconfirmed. Reopen this world to validate saved progress."),
                Phase::Loading => ("Preparing your world", "Terrain, local population and saved progress are being prepared."),
                Phase::Saving { .. } => ("Saving progress", "Waiting for a successful checkpoint before leaving or resuming."),
                Phase::Paused(Page::Settings) => ("Session settings", "Changes apply immediately. Save settings to keep them for the next launch."),
                Phase::Paused(Page::RestoreReview) => ("Restore this checkpoint?", "Changes after the selected snapshot will be discarded. Cancel keeps your current progress."),
                Phase::Paused(Page::Controls) => ("Controls", "Command a group or step into the world as a single actor."),
                _ if runtime.network_authority => ("Session menu", "The shared world keeps running while this menu is open."),
                _ => ("World paused", "Your local simulation is paused. Its clock settings stay intact."),
            };
            p.spawn(label(title, 30., TEXT));
            p.spawn(label(options.world_title.as_deref().unwrap_or("Terra World"), 14., ACCENT));
            p.spawn(label(subtitle, 14., MUTED));
            scroll_panel(p, 61., 10., |p| {
                p.spawn((StatusText, label("", 13., MUTED)));
                match phase {
                    Phase::Loading | Phase::Paused(Page::Session) => {
                        if terminal_surface { p.spawn(label(TERMINAL_STORAGE_NOTICE, 14., Color::srgb(0.94, 0.72, 0.54))); }
                        for (title, action) in &choices { button(p, *title, *action, initial); }
                    },
                    Phase::Paused(Page::RestoreReview) => {
                        if let Some(review) = &flow.restore_review {
                            p.spawn(label(review.description(), 15., ACCENT)).insert(TextLayout::linebreak(bevy::text::LineBreak::WordOrCharacter));
                        }
                        button(p, "Cancel · Keep current progress", Action::Session, initial);
                        button(p, "Restore reviewed checkpoint · Discard later changes", Action::ConfirmRestore(flow.restore_serial), initial);
                    },
                    Phase::Saving { .. } => { p.spawn(label("Do not close the window while the checkpoint is working. If storage fails, this menu keeps your session open and offers recovery.", 14., MUTED)); },
                    Phase::Paused(Page::Settings) => {
                        button(p, format!("Graphics quality · {}", quality.quality.label()), Action::Quality, initial);
                        button(p, format!("Atmospheric sky · {}{}", if quality.uses_atmosphere() { "On" } else { "Off" }, if quality.quality == terra_bevy::render::RenderQuality::Performance { format!(" · Performance\nSky preference · {}", if quality.atmospheric_sky { "On" } else { "Off" }) } else { String::new() }), Action::Sky, initial);
                        if !runtime.network_authority {
                            button(p, format!("Day and night · {}", if runtime.config.day_night { "On" } else { "Off" }), Action::DayNight, initial);
                            button(p, format!("Seasonal visuals · {}", if runtime.config.seasons { "On" } else { "Off" }), Action::Seasons, initial);
                            button(p, format!("World time speed · {:.0}×", runtime.clock.time_scale), Action::TimeScale, initial);
                        }
                        p.spawn(label("Performance lowers presentation cost and disables atmospheric scattering. Source terrain, collision and gameplay rules are unchanged.", 13., MUTED));
                        button(p, "Save settings", Action::SaveSettings, initial);
                        button(p, "Back to session", Action::Session, initial);
                    },
                    Phase::Paused(Page::Controls) => {
                        p.spawn(label("F1 · RTS   F2 · Third person   F3 · First person\nClick / drag to select · Right-click to command\nJ · Activities   N · Nature   T · Territory   Y · Fleet\nV · Driving   G · Garage   M · Atlas\nF5 · Checkpoint   F9 · Restore   F10–F12 · World / Inspector / Performance\nTab / Shift+Tab · Menu focus   Enter · Activate", 14., TEXT));
                        p.spawn(label(&input_settings.notice, 14., ACCENT));
                        for (title, action) in input_settings.rows() { button(p, title, Action::InputSetting(action), initial); }
                        p.spawn(label(format!("{} controller(s) detected{}", input.connected_gamepads, if input.controller_name.is_empty() { String::new() } else { format!(" · {}", input.controller_name) }), 13., MUTED)).insert(crate::player_input::ControllerStatus);
                        p.spawn(label(crate::player_input::GAMEPAD_HELP, 14., TEXT));
                    },
                    _ => {},
                }
                if let Some(notice) = &flow.notice { p.spawn(label(notice, 14., Color::srgb(0.94, 0.72, 0.54))); }
            });
            if matches!(phase, Phase::Paused(Page::Controls)) {
                p.spawn(Node { width: percent(100), column_gap: px(8), flex_shrink: 0., ..default() }).with_children(|p| {
                    for (title, action) in [
                        ("Back to session", Action::Session),
                        ("Save controls", Action::InputSetting(crate::player_input::SettingAction::Save)),
                    ] {
                        p.spawn(Node { flex_grow: 1., flex_basis: px(0), min_width: px(0), ..default() }).with_children(|p| button(p, title, action, initial));
                    }
                });
            }
            p.spawn(label("Tab to navigate · Enter to choose · Mouse wheel to scroll", 12., MUTED));
        });
    });
}

fn update_status(
    flow: Res<GameFlow>,
    runtime: Res<WorldRuntime>,
    resident: Res<ResidentChunks>,
    render: Res<RenderTelemetry>,
    sandbox: Res<SandboxRuntime>,
    persistence: Res<RegionalPersistence>,
    network: Option<Res<NetworkStatus>>,
    living: Option<Res<LivingDiagnostics>>,
    options: Res<DemoOptions>,
    capture: Option<Res<crate::performance::CaptureStatus>>,
    mut labels: Query<&mut Text, With<StatusText>>,
) {
    if !flow.blocks_input() {
        return;
    }
    let mut status = if flow.phase == Phase::Loading && !persistence.requires_reopen() {
        let (wanted, sourced, rendered) =
            loading_terrain_counts(&runtime, &sandbox, &resident, &render);
        format!(
            "Nearby terrain · {}% ready   /   Source data · {}% ready\n{}\n{} actors preparing for the world",
            rendered.min(wanted) * 100 / wanted.max(1),
            sourced.min(wanted) * 100 / wanted.max(1),
            crate::launcher::model::region_coverage_label(
                loading_position(&runtime, &sandbox),
                runtime.config.chunk_level,
                runtime.config.chunk_radius
            ),
            sandbox.state.living.units.len(),
        )
    } else {
        runtime.clock.label()
    };
    if let Some(network) = network {
        status.push_str(&format!("\n{}", network.message));
        if network.connected {
            status.push_str(if network.encrypted {
                " · Verified TLS"
            } else {
                " · Plain TCP"
            });
        }
        status.push('\n');
        status.push_str(save_contract_description(true, false, false));
    } else if let Some(checkpoint) = &persistence.status {
        status.push_str(&format!(
            "\nLast saved checkpoint · {}{}",
            checkpoint.generation,
            if persistence.pending() {
                " · Saving…"
            } else {
                ""
            }
        ));
    } else if persistence.enabled {
        status.push_str("\nPreparing first checkpoint…");
    } else {
        status.push('\n');
        status.push_str(save_contract_description(
            runtime.network_authority,
            persistence.enabled,
            options.world_profile_path.is_some(),
        ));
    }
    if let Some(error) = &persistence.last_error {
        status.push_str(&format!("\nStorage error: {error}"));
    }
    if let Some(error) = &runtime.last_error {
        status.push_str(&format!("\n{error}"));
    }
    if let Some(error) = living.and_then(|d| d.last_error.clone()) {
        status.push_str(&format!("\n{error}"));
    }
    if matches!(flow.phase, Phase::Paused(Page::Session))
        && !sandbox.last_message.starts_with("Checkpoint ")
        && !sandbox.last_message.starts_with("Restored checkpoint ")
    {
        status.push_str(&format!("\n{}", sandbox.last_message));
    }
    if let Some(capture) = capture {
        status.push_str(&format!("\n{}", capture.describe(&options)));
    }
    for mut text in &mut labels {
        if text.0 != status {
            text.0 = status.clone();
        }
    }
}

/// Player-facing persistence policy shared by the session menu and World drawer.
/// Managed portable worlds have a clean-exit hook; direct-play JSON deliberately
/// keeps its explicit-save contract.
pub(crate) fn save_contract_description(
    network_authority: bool,
    regional: bool,
    managed_world: bool,
) -> &'static str {
    if network_authority {
        "Your host owns progress. This client cannot save or restore the shared world."
    } else if regional {
        "Regional checkpoints · Automatic saves and clean exits preserve progress. F5 checkpoints now; F9 reviews restoring the latest checkpoint and discarding later changes."
    } else if managed_world {
        "Portable save · F5 and clean exit save progress. No periodic autosave. F9 reviews restoring the saved snapshot and discarding later changes."
    } else {
        "Manual JSON save · Use F5 or Save and quit / return to main menu. Closing the window does not save progress. No periodic autosave. F9 reviews restoring the saved snapshot and discarding later changes."
    }
}

fn style(
    focus: Res<InputFocus>,
    mut buttons: Query<
        (
            Entity,
            &Hovered,
            Has<Pressed>,
            &mut BackgroundColor,
            &mut BorderColor,
        ),
        With<FlowButton>,
    >,
) {
    for (entity, hover, pressed, mut background, mut border) in &mut buttons {
        background.0 = if pressed {
            Color::srgb(0.22, 0.357, 0.263)
        } else if hover.get() {
            Color::srgb(0.165, 0.282, 0.231)
        } else {
            SURFACE
        };
        *border = BorderColor::all(if focus.get() == Some(entity) {
            ACCENT
        } else {
            Color::NONE
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;

    fn loading_chunk(key: terra_core::TileKey) -> std::sync::Arc<terra_core::WorldChunk> {
        std::sync::Arc::new(terra_core::WorldChunk {
            key,
            heights: terra_core::HeightField::constant(100.),
            features: Vec::new(),
            sources: Vec::new(),
            warnings: Vec::new(),
        })
    }

    fn loading_app(latitude: f64, radius: u32) -> (App, Vec<terra_core::TileKey>) {
        let position = terra_core::TileKey::at(
            terra_core::GeoPosition::new(latitude, 11.575, 100.).unwrap(),
            10,
        )
        .unwrap()
        .center();
        let config = terra_bevy::WorldConfig {
            focus: position,
            chunk_level: 10,
            chunk_radius: radius,
            ..default()
        };
        let keys = terra_bevy::living::startup_neighborhood(&config, position);
        let mut sandbox = SandboxRuntime::new(position);
        sandbox
            .state
            .seed_living_demo(position, &[], &sandbox.catalog)
            .unwrap();
        let mut app = App::new();
        app.insert_resource(WorldRuntime {
            focus: position,
            frame: terra_core::LocalFrame::new(position),
            config,
            origin_revision: 0,
            clock: Default::default(),
            weather: Default::default(),
            stream_stats: Default::default(),
            last_error: None,
            stream_poll_ms: 0.,
            simulation_ms: 0.,
            rebases: 0,
            network_authority: false,
        })
        .insert_resource(sandbox)
        .insert_resource(ResidentChunks {
            level: 10,
            chunks: keys.iter().map(|&key| (key, loading_chunk(key))).collect(),
        })
        .insert_resource(RenderTelemetry {
            chunks: keys.len(),
            integrated_chunks: keys.iter().copied().collect(),
            ..default()
        })
        .init_resource::<GameFlow>()
        .init_resource::<SandboxQueue>()
        .init_resource::<RegionalPersistence>()
        .init_resource::<DemoOptions>()
        .add_message::<AppExit>()
        .add_systems(Update, poll_state);
        app.world_mut().spawn((StatusText, Text::new("")));
        (app, keys)
    }

    fn loading_status(app: &mut App) -> String {
        app.world_mut().run_system_once(update_status).unwrap();
        app.world_mut()
            .query_filtered::<&Text, With<StatusText>>()
            .single(app.world())
            .unwrap()
            .0
            .clone()
    }

    #[test]
    fn native_loading_uses_real_polar_ordinary_and_single_tile_neighborhoods() {
        for (latitude, radius, count) in
            [(89.9, 1, 6), (-89.9, 1, 6), (48.137, 1, 9), (48.137, 0, 1)]
        {
            let (mut app, keys) = loading_app(latitude, radius);
            assert_eq!(keys.len(), count);
            assert!(
                loading_status(&mut app)
                    .starts_with("Nearby terrain · 100% ready   /   Source data · 100% ready")
            );
            // The startup source follows the player's real position rather
            // than an unrelated spectator focus, matching population bootstrap.
            app.world_mut().resource_mut::<WorldRuntime>().focus =
                terra_core::GeoPosition::new(0., -80., 0.).unwrap();
            app.update();
            assert_eq!(
                app.world().resource::<GameFlow>().phase,
                Phase::Playing,
                "{latitude}: complete real neighborhood must admit play"
            );
            assert!(app.world().resource::<SandboxQueue>().0.is_empty());
        }
    }

    #[test]
    fn native_loading_rejects_unrelated_or_unrendered_pages_despite_total_counts() {
        for source_missing in [false, true] {
            let (mut app, keys) = loading_app(48.137, 1);
            let held = keys[0];
            let unrelated = terra_core::TileKey::new(held.level, held.x + 30, held.y).unwrap();
            if source_missing {
                let mut resident = app.world_mut().resource_mut::<ResidentChunks>();
                resident.chunks.remove(&held);
                resident.chunks.insert(unrelated, loading_chunk(unrelated));
                // A stale rendered key without its current resident source
                // cannot satisfy the intersection either.
            } else {
                let mut render = app.world_mut().resource_mut::<RenderTelemetry>();
                render.integrated_chunks.remove(&held);
                render.integrated_chunks.insert(unrelated);
            }
            assert_eq!(app.world().resource::<ResidentChunks>().chunks.len(), 9);
            assert_eq!(app.world().resource::<RenderTelemetry>().chunks, 9);
            let status = loading_status(&mut app);
            assert!(status.starts_with("Nearby terrain · 88% ready"));
            assert!(status.contains(if source_missing {
                "Source data · 88% ready"
            } else {
                "Source data · 100% ready"
            }));
            app.update();
            assert_eq!(app.world().resource::<GameFlow>().phase, Phase::Loading);
            app.world_mut()
                .resource_mut::<ResidentChunks>()
                .chunks
                .insert(held, loading_chunk(held));
            app.world_mut()
                .resource_mut::<RenderTelemetry>()
                .integrated_chunks
                .insert(held);
            assert!(
                loading_status(&mut app)
                    .starts_with("Nearby terrain · 100% ready   /   Source data · 100% ready")
            );
            app.update();
            assert_eq!(app.world().resource::<GameFlow>().phase, Phase::Playing);
        }
    }

    #[test]
    fn native_loading_keeps_account_network_and_nonempty_neighborhood_gates() {
        let (mut app, _) = loading_app(89.9, 1);
        let account = app
            .world_mut()
            .resource_mut::<SandboxRuntime>()
            .state
            .living
            .accounts
            .remove(&1)
            .unwrap();
        app.update();
        assert_eq!(app.world().resource::<GameFlow>().phase, Phase::Loading);
        app.world_mut()
            .resource_mut::<SandboxRuntime>()
            .state
            .living
            .accounts
            .insert(1, account);
        app.world_mut()
            .resource_mut::<WorldRuntime>()
            .network_authority = true;
        app.init_resource::<NetworkStatus>();
        app.update();
        assert_eq!(app.world().resource::<GameFlow>().phase, Phase::Loading);
        app.world_mut().resource_mut::<NetworkStatus>().connected = true;
        app.update();
        assert_eq!(app.world().resource::<GameFlow>().phase, Phase::Loading);
        app.world_mut().resource_mut::<NetworkStatus>().actor_id = Some(1);
        app.update();
        assert_eq!(app.world().resource::<GameFlow>().phase, Phase::Playing);

        let (mut invalid, _) = loading_app(48.137, 1);
        invalid
            .world_mut()
            .resource_mut::<WorldRuntime>()
            .config
            .chunk_level = u8::MAX;
        assert!(
            loading_status(&mut invalid)
                .starts_with("Nearby terrain · 0% ready   /   Source data · 0% ready")
        );
        invalid.update();
        assert_eq!(invalid.world().resource::<GameFlow>().phase, Phase::Loading);

        let (mut awaiting_player, _) = loading_app(48.137, 0);
        awaiting_player
            .world_mut()
            .resource_mut::<SandboxRuntime>()
            .state
            .players
            .clear();
        awaiting_player
            .world_mut()
            .resource_mut::<SandboxRuntime>()
            .state
            .living
            .accounts
            .clear();
        assert!(
            loading_status(&mut awaiting_player)
                .starts_with("Nearby terrain · 100% ready   /   Source data · 100% ready")
        );
        awaiting_player.update();
        assert_eq!(
            awaiting_player.world().resource::<GameFlow>().phase,
            Phase::Loading
        );
    }

    #[test]
    fn loading_controller_reaches_cancel_save_and_terminal_recovery_without_world_input() {
        use crate::{
            accessibility::AccessibleUiPlugin,
            player_input::{InputSettings, PlayerInput},
        };
        for (network, terminal) in [(true, false), (false, false), (false, true)] {
            let mut app = App::new();
            let config = terra_bevy::WorldConfig::default();
            let position = config.focus;
            app.insert_resource(WorldRuntime {
                focus: position,
                frame: terra_core::LocalFrame::new(position),
                config,
                origin_revision: 0,
                clock: Default::default(),
                weather: Default::default(),
                stream_stats: Default::default(),
                last_error: None,
                stream_poll_ms: 0.,
                simulation_ms: 0.,
                rebases: 0,
                network_authority: network,
            })
            .insert_resource(SandboxRuntime::new(position))
            .init_resource::<SandboxQueue>()
            .init_resource::<RegionalPersistence>()
            .init_resource::<RenderQualitySettings>()
            .init_resource::<DemoOptions>()
            .init_resource::<GameFlow>()
            .init_resource::<Actions>()
            .init_resource::<InputSettings>()
            .init_resource::<PlayerInput>()
            .init_resource::<InputFocus>()
            .init_resource::<bevy::input_focus::InputFocusVisible>()
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<Time<Real>>()
            .add_message::<AppExit>()
            .add_plugins(AccessibleUiPlugin);
            let (title, action) = session_buttons(Phase::Loading, network, terminal)[0];
            app.world_mut()
                .commands()
                .spawn((Node::default(), TabGroup::modal()))
                .with_children(|p| {
                    button(p, title, action, None);
                });
            app.world_mut().flush();
            app.update();
            let control = app
                .world_mut()
                .query_filtered::<Entity, With<FlowButton>>()
                .single(app.world())
                .unwrap();
            app.world_mut().entity_mut(control).insert((
                ComputedNode {
                    size: Vec2::new(400., 40.),
                    ..default()
                },
                InheritedVisibility::VISIBLE,
            ));
            app.world_mut()
                .resource_mut::<InputFocus>()
                .set(control, bevy::input_focus::FocusCause::Navigated);
            app.world_mut().resource_mut::<PlayerInput>().menu_direction = IVec2::Y;
            app.update();
            assert_eq!(app.world().resource::<InputFocus>().get(), Some(control));
            assert!(app.world().resource::<GameFlow>().blocks_input());
            app.world_mut().resource_mut::<PlayerInput>().menu_accept = true;
            app.update();
            assert_eq!(app.world().resource::<Actions>().0.front(), Some(&action));
            assert!(app.world().resource::<SandboxQueue>().0.is_empty());
            if terminal {
                let activated = app
                    .world_mut()
                    .resource_mut::<Actions>()
                    .0
                    .pop_front()
                    .unwrap();
                let mut flow = app.world_mut().resource_mut::<GameFlow>();
                assert_eq!(
                    terminal_storage_action(activated, false, true, &mut flow),
                    TerminalStorageAction::Close
                );
                assert!(flow.return_to_menu);
                assert!(flow.blocks_input());
            } else {
                *app.world_mut().resource_mut::<PlayerInput>() = PlayerInput::default();
                app.add_systems(
                    Update,
                    process_actions.after(crate::accessibility::ControllerNavigation),
                );
                app.update();
                if network {
                    assert_eq!(app.should_exit(), Some(AppExit::Success));
                    assert!(app.world().resource::<GameFlow>().return_to_menu);
                    assert!(app.world().resource::<SandboxQueue>().0.is_empty());
                } else {
                    assert_eq!(app.should_exit(), None);
                    assert!(matches!(
                        app.world().resource::<GameFlow>().phase,
                        Phase::Saving {
                            destination: Destination::Menu,
                            ..
                        }
                    ));
                    assert!(matches!(
                        app.world().resource::<SandboxQueue>().0.front(),
                        Some(SandboxIntent::Save)
                    ));
                }
            }
        }
        for (phase, menu, blocked) in [
            (Phase::Playing, false, false),
            (Phase::Paused(Page::Session), true, true),
            (
                Phase::Saving {
                    destination: Destination::Stay,
                    generation: 0,
                },
                false,
                true,
            ),
        ] {
            let flow = GameFlow { phase, ..default() };
            assert_eq!(flow.controller_menu_active(), menu);
            assert_eq!(flow.blocks_input(), blocked);
        }
    }

    #[test]
    fn session_button_rejects_native_activation_during_text_entry_and_its_closing_frame() {
        use crate::{
            accessibility::ControllerKeyboard,
            player_input::{InputSettings, PlayerInput},
        };
        use bevy::{
            input::keyboard::{Key, KeyboardInput},
            input_focus::FocusedInput,
            ui_widgets::ButtonPlugin,
        };
        let mut app = App::new();
        app.init_resource::<Actions>()
            .init_resource::<InputSettings>()
            .init_resource::<PlayerInput>()
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<ControllerKeyboard>()
            .add_plugins(ButtonPlugin);
        app.world_mut()
            .commands()
            .spawn(Node::default())
            .with_children(|p| button(p, "Resume", Action::Resume, None));
        app.world_mut().flush();
        app.update();
        let target = app
            .world_mut()
            .query_filtered::<Entity, With<FlowButton>>()
            .single(app.world())
            .unwrap();
        app.world_mut()
            .resource_mut::<ControllerKeyboard>()
            .open_world(crate::interface::InputMode::Coordinates, "");
        for closing in [false, true] {
            if closing {
                *app.world_mut().resource_mut::<ControllerKeyboard>() =
                    ControllerKeyboard::default();
                app.world_mut()
                    .resource_mut::<ControllerKeyboard>()
                    .consumed = true;
            }
            for (key_code, logical_key) in
                [(KeyCode::Enter, Key::Enter), (KeyCode::Space, Key::Space)]
            {
                app.world_mut().trigger(FocusedInput::new(
                    target,
                    KeyboardInput {
                        key_code,
                        logical_key,
                        state: bevy::input::ButtonState::Pressed,
                        text: None,
                        repeat: false,
                        window: target,
                    },
                    target,
                ));
                // ButtonPlugin queues Activate through Commands. Flush before
                // changing the guard state or inspecting its action queue.
                app.world_mut().flush();
            }
            assert!(app.world().resource::<Actions>().0.is_empty());
        }
        app.world_mut()
            .resource_mut::<ControllerKeyboard>()
            .consumed = false;
        app.world_mut().trigger(FocusedInput::new(
            target,
            KeyboardInput {
                key_code: KeyCode::Enter,
                logical_key: Key::Enter,
                state: bevy::input::ButtonState::Pressed,
                text: None,
                repeat: false,
                window: target,
            },
            target,
        ));
        app.world_mut().flush();
        assert_eq!(
            app.world().resource::<Actions>().0.front(),
            Some(&Action::Resume)
        );
    }

    #[test]
    fn save_contract_copy_matches_all_four_session_modes() {
        for (network, regional, managed, required, forbidden) in [
            (
                false,
                false,
                true,
                vec!["clean exit", "No periodic autosave", "F5"],
                vec!["Closing the window does not save"],
            ),
            (
                false,
                false,
                false,
                vec![
                    "F5",
                    "Save and quit / return to main menu",
                    "Closing the window does not save",
                    "No periodic autosave",
                ],
                vec!["clean exit save"],
            ),
            (
                false,
                true,
                true,
                vec!["Regional checkpoints", "Automatic saves", "F9"],
                vec!["No periodic autosave"],
            ),
            (
                true,
                true,
                true,
                vec!["host owns progress", "cannot save or restore"],
                vec!["clean exit", "F5"],
            ),
        ] {
            let copy = save_contract_description(network, regional, managed);
            for word in required {
                assert!(copy.contains(word), "{copy}");
            }
            for word in forbidden {
                assert!(!copy.contains(word), "{copy}");
            }
        }
    }

    #[test]
    fn terminal_recovery_menu_matches_its_guard_and_does_not_retry_uncertain_work() {
        for phase in [Phase::Loading, Phase::Paused(Page::Session)] {
            let choices = session_buttons(phase, false, true);
            assert!(choices.contains(&("Close failed world", Action::CloseFailedWorld)));
            assert!(!choices.iter().any(|(_, action)| matches!(
                action,
                Action::Resume
                    | Action::Checkpoint
                    | Action::Restore
                    | Action::ConfirmRestore(_)
                    | Action::Menu
                    | Action::Quit
            )));
            let mut flow = GameFlow {
                phase,
                ..Default::default()
            };
            for action in [
                Action::Resume,
                Action::Checkpoint,
                Action::Restore,
                Action::Menu,
                Action::Quit,
            ] {
                assert_eq!(
                    terminal_storage_action(action, false, true, &mut flow),
                    TerminalStorageAction::RemainOpen
                );
                assert_eq!(
                    flow.phase, phase,
                    "no impossible checkpoint wait is entered"
                );
                assert!(!flow.return_to_menu);
                assert!(flow.notice.as_deref().unwrap().contains("unconfirmed"));
            }
            assert_eq!(
                terminal_storage_action(Action::CloseFailedWorld, false, true, &mut flow),
                TerminalStorageAction::Close
            );
            assert!(flow.return_to_menu);
            assert!(
                flow.notice
                    .as_deref()
                    .unwrap()
                    .contains("may have committed")
            );
        }
        // The same queued recovery action cannot abandon a normal/recoverable
        // local world, a host-owned world or an unrelated settings screen.
        for (network, requires_reopen, phase) in [
            (false, false, Phase::Paused(Page::Session)),
            (true, true, Phase::Paused(Page::Session)),
            (false, true, Phase::Paused(Page::Settings)),
        ] {
            let mut flow = GameFlow {
                phase,
                ..Default::default()
            };
            assert_eq!(
                terminal_storage_action(
                    Action::CloseFailedWorld,
                    network,
                    requires_reopen,
                    &mut flow
                ),
                TerminalStorageAction::RemainOpen
            );
            assert!(!flow.return_to_menu);
            assert!(
                !session_buttons(phase, network, requires_reopen)
                    .iter()
                    .any(|(_, action)| *action == Action::CloseFailedWorld)
            );
        }
        let normal = session_buttons(Phase::Paused(Page::Session), false, false);
        for action in [
            Action::Checkpoint,
            Action::Restore,
            Action::Menu,
            Action::Quit,
        ] {
            assert!(normal.iter().any(|(_, choice)| *choice == action));
            assert_eq!(
                terminal_storage_action(action, false, false, &mut GameFlow::default()),
                TerminalStorageAction::Normal
            );
        }
    }

    #[test]
    fn ordinary_session_keeps_save_restore_retry_and_quit_after_rejecting_terminal_close() {
        let directory = std::env::temp_dir().join(format!(
            "terra-flow-recovery-menu-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&directory).unwrap();
        let save_path = directory.join("world.json");
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_plugins(terra_bevy::TerraWorldPlugin::new(terra_bevy::WorldConfig {
                chunk_radius: 0,
                terrain_resolution: 9,
                time_scale: 0.,
                live_weather: false,
                cache_dir: directory.join("cache"),
                save_path: save_path.clone(),
                ..default()
            }))
            .init_resource::<GameFlow>()
            .init_resource::<Actions>()
            .init_resource::<crate::player_input::InputSettings>()
            .init_resource::<RenderQualitySettings>()
            .init_resource::<RenderTelemetry>()
            .init_resource::<DemoOptions>()
            .add_systems(Update, process_actions.before(WorldSet::Sandbox))
            .add_systems(Update, poll_state.after(WorldSet::Render));
        app.world_mut()
            .resource_mut::<GameFlow>()
            .change(Phase::Paused(Page::Session));
        app.world_mut()
            .resource_mut::<Actions>()
            .0
            .push_back(Action::CloseFailedWorld);
        app.update();
        assert_eq!(app.should_exit(), None);
        assert!(!save_path.exists());
        assert!(app.world().resource::<SandboxQueue>().0.is_empty());
        assert!(
            app.world()
                .resource::<GameFlow>()
                .notice
                .as_deref()
                .unwrap()
                .contains("normal save")
        );

        app.world_mut()
            .resource_mut::<Actions>()
            .0
            .push_back(Action::Checkpoint);
        app.update();
        assert!(save_path.is_file());
        assert_eq!(
            app.world().resource::<GameFlow>().phase,
            Phase::Paused(Page::Session)
        );
        let saved_position = app.world().resource::<SandboxRuntime>().state.players[&1].position;
        let revision = app.world().resource::<SandboxRuntime>().restore_revision;
        let changed_position = terra_core::LocalFrame::new(saved_position)
            .to_geo([20., 0., 0.])
            .unwrap();
        app.world_mut()
            .resource_mut::<SandboxRuntime>()
            .state
            .set_player_position(1, changed_position)
            .unwrap();
        app.world_mut()
            .resource_mut::<Actions>()
            .0
            .push_back(Action::Restore);
        app.update();
        assert_eq!(
            app.world().resource::<GameFlow>().phase,
            Phase::Paused(Page::RestoreReview)
        );
        app.world_mut()
            .resource_mut::<Actions>()
            .0
            .push_back(Action::Session);
        app.update();
        assert_eq!(
            app.world().resource::<SandboxRuntime>().state.players[&1].position,
            changed_position
        );
        assert_eq!(
            app.world().resource::<SandboxRuntime>().restore_revision,
            revision
        );
        // This is the same request method used by the in-world Load/F9 action.
        app.world_mut().resource_mut::<GameFlow>().request_restore();
        app.update();
        assert_eq!(
            app.world().resource::<GameFlow>().phase,
            Phase::Paused(Page::RestoreReview)
        );
        assert!(app.world().resource::<GameFlow>().focus.is_none());

        assert_eq!(
            app.world().resource::<SandboxRuntime>().state.players[&1].position,
            changed_position
        );
        let old_serial = app.world().resource::<GameFlow>().restore_serial;
        app.world_mut().resource_mut::<Actions>().0.extend([
            Action::Session,
            Action::Restore,
            Action::ConfirmRestore(old_serial),
        ]);
        app.update();
        assert_eq!(
            app.world().resource::<GameFlow>().phase,
            Phase::Paused(Page::RestoreReview)
        );
        assert!(app.world().resource::<GameFlow>().restore_serial > old_serial);
        assert!(app.world().resource::<GameFlow>().focus.is_none());
        assert_eq!(
            app.world().resource::<SandboxRuntime>().state.players[&1].position,
            changed_position
        );
        let serial = app.world().resource::<GameFlow>().restore_serial;
        app.world_mut()
            .resource_mut::<Actions>()
            .0
            .push_back(Action::ConfirmRestore(serial));
        app.update();
        assert_eq!(
            app.world().resource::<SandboxRuntime>().state.players[&1].position,
            saved_position
        );
        assert!(app.world().resource::<SandboxRuntime>().restore_revision > revision);
        let restored_revision = app.world().resource::<SandboxRuntime>().restore_revision;
        let serial = app.world().resource::<GameFlow>().restore_serial;
        app.world_mut()
            .resource_mut::<Actions>()
            .0
            .push_back(Action::ConfirmRestore(serial));
        app.update();
        assert_eq!(
            app.world().resource::<SandboxRuntime>().restore_revision,
            restored_revision
        );

        // An ordinary write error remains recoverable in the same session. This
        // uses a disposable real filesystem obstruction, not a worker failpoint.
        std::fs::remove_file(&save_path).unwrap();
        std::fs::create_dir(&save_path).unwrap();
        app.world_mut()
            .resource_mut::<Actions>()
            .0
            .push_back(Action::Checkpoint);
        app.update();
        assert_eq!(app.should_exit(), None);
        assert_eq!(
            app.world().resource::<GameFlow>().phase,
            Phase::Paused(Page::Session)
        );
        assert!(
            app.world()
                .resource::<GameFlow>()
                .notice
                .as_deref()
                .unwrap()
                .contains("Checkpoint failed")
        );
        assert!(save_path.is_dir());
        std::fs::remove_dir(&save_path).unwrap();
        app.world_mut()
            .resource_mut::<Actions>()
            .0
            .push_back(Action::Checkpoint);
        app.update();
        assert!(save_path.is_file());
        assert!(
            app.world()
                .resource::<GameFlow>()
                .notice
                .as_deref()
                .unwrap()
                .contains("Checkpoint complete")
        );
        app.world_mut()
            .resource_mut::<Actions>()
            .0
            .push_back(Action::Quit);
        app.update();
        assert_eq!(app.should_exit(), Some(AppExit::Success));
        assert!(!app.world().resource::<GameFlow>().return_to_menu);
        drop(app);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn binding_capture_cannot_restore_and_leaving_controls_cancels_capture() {
        use crate::player_input::{Binding, InputSettings, PlayerInput};
        let directory = std::env::temp_dir().join(format!(
            "terra-flow-binding-capture-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut builder = std::fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder.create(&directory).unwrap();
        let save_path = directory.join("world.json");
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_plugins(terra_bevy::TerraWorldPlugin::new(terra_bevy::WorldConfig {
                chunk_radius: 0,
                terrain_resolution: 9,
                live_weather: false,
                cache_dir: directory.join("cache"),
                save_path: save_path.clone(),
                ..default()
            }))
            .init_resource::<GameFlow>()
            .init_resource::<Actions>()
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<PlayerInput>()
            .init_resource::<InputSettings>()
            .init_resource::<RenderQualitySettings>()
            .init_resource::<DemoOptions>()
            .add_systems(Update, (keyboard_and_pause, process_actions).chain());
        app.world_mut()
            .resource_mut::<GameFlow>()
            .change(Phase::Paused(Page::Controls));
        app.world_mut().resource_mut::<InputSettings>().listening = Some(Binding::Forward);
        app.world_mut().resource_mut::<PlayerInput>().suppress = true;
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::F9);
        app.update();
        assert_eq!(
            app.world().resource::<GameFlow>().phase,
            Phase::Paused(Page::Controls)
        );
        assert!(app.world().resource::<GameFlow>().notice.is_none());
        assert!(app.world().resource::<SandboxQueue>().0.is_empty());
        assert_eq!(
            app.world().resource::<InputSettings>().listening,
            Some(Binding::Forward)
        );
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .reset_all();
        app.world_mut()
            .resource_mut::<Actions>()
            .0
            .push_back(Action::Session);
        app.update();
        assert_eq!(
            app.world().resource::<GameFlow>().phase,
            Phase::Paused(Page::Session)
        );
        assert!(app.world().resource::<InputSettings>().listening.is_none());
        app.world_mut().resource_mut::<PlayerInput>().suppress = false;
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::F5);
        app.update();
        assert!(matches!(
            app.world().resource::<GameFlow>().phase,
            Phase::Saving { .. }
        ));
        // The ordinary save consumer can run before or after this test's UI
        // systems. Drain the request and prove that its real write uses the
        // isolated destination rather than the crate's default world-save.json.
        app.update();
        assert!(app.world().resource::<SandboxQueue>().0.is_empty());
        assert!(save_path.is_file());
        drop(app);
        std::fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn app_run_retains_final_handoff_after_consuming_the_world() {
        let (handoff, receiver) = exit_handoff();
        let mut app = App::new();
        app.add_plugins(MinimalPlugins.set(bevy::app::ScheduleRunnerPlugin::run_once()))
            .insert_resource(handoff)
            .insert_resource(DemoOptions {
                user_data: PathBuf::from("managed-worlds"),
                world_profile_path: Some(PathBuf::from("managed-worlds/world.json")),
                menu_supervised: true,
                ..default()
            })
            .init_resource::<GameFlow>()
            .init_resource::<RenderQualitySettings>()
            .init_resource::<crate::player_input::InputSettings>()
            .add_plugins(terra_bevy::TerraWorldPlugin::new(terra_bevy::WorldConfig {
                chunk_radius: 0,
                terrain_resolution: 9,
                live_weather: false,
                ..default()
            }))
            .add_systems(Update, |mut exit: MessageWriter<AppExit>| {
                exit.write(AppExit::Success);
            })
            .add_systems(
                Last,
                (|mut runtime: ResMut<WorldRuntime>,
                  mut quality: ResMut<RenderQualitySettings>| {
                    runtime.config.time_scale = 15.;
                    quality.quality = terra_bevy::render::RenderQuality::High;
                })
                .in_set(terra_bevy::RegionPersistenceSet::FinalCheckpoint),
            )
            .add_systems(
                Last,
                capture_exit_handoff
                    .in_set(bevy::app::OnAppExitSystems)
                    .after(terra_bevy::RegionPersistenceSet::FinalCheckpoint),
            );
        app.world_mut().resource_mut::<GameFlow>().return_to_menu = true;
        assert_eq!(app.run(), AppExit::Success);
        // This is the behavior that made reading resources after run panic.
        assert!(app.world().get_resource::<DemoOptions>().is_none());
        let snapshot = receiver
            .try_recv()
            .expect("final handoff survives the consumed app");
        assert!(snapshot.return_to_menu && snapshot.menu_supervised);
        assert_eq!(snapshot.user_data, PathBuf::from("managed-worlds"));
        assert_eq!(
            snapshot.profile,
            Some(PathBuf::from("managed-worlds/world.json"))
        );
        assert_eq!(snapshot.config.time_scale, 15.);
        assert_eq!(
            snapshot.settings.quality,
            terra_bevy::render::RenderQuality::High
        );
        assert!(
            receiver.try_recv().is_err(),
            "only one bounded handoff is retained"
        );
    }

    #[test]
    fn escape_cancels_the_immediate_edit_before_pausing_and_consumes_its_frame() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_plugins(terra_bevy::TerraWorldPlugin::new(terra_bevy::WorldConfig {
                chunk_radius: 0,
                terrain_resolution: 9,
                live_weather: false,
                ..default()
            }))
            .init_resource::<GameFlow>()
            .init_resource::<Actions>()
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<crate::player_input::PlayerInput>()
            .init_resource::<crate::interface::UiState>()
            .init_resource::<crate::interaction::Placement>()
            .add_systems(Update, keyboard_and_pause);
        app.world_mut()
            .resource_mut::<GameFlow>()
            .change(Phase::Playing);
        app.world_mut()
            .resource_mut::<crate::interface::UiState>()
            .editing_gps = true;
        app.world_mut()
            .resource_mut::<crate::interaction::Placement>()
            .0 = Some(crate::interaction::PlaceKind::Crop);
        {
            let mut politics = app.world_mut().resource_mut::<terra_bevy::PoliticalWorld>();
            politics.drawing = true;
            politics
                .draft
                .push(terra_core::GeoPosition::new(48., 11., 0.).unwrap());
        }
        for step in 0..3 {
            {
                let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
                keys.reset_all();
                keys.press(KeyCode::Escape);
            }
            app.update();
            let flow = app.world().resource::<GameFlow>();
            assert_eq!(flow.phase, Phase::Playing);
            assert!(flow.blocks_input(), "cancel consumes the closing frame");
            assert!(
                !app.world()
                    .resource::<crate::interface::UiState>()
                    .editing_gps
            );
            assert_eq!(
                app.world()
                    .resource::<crate::interaction::Placement>()
                    .is_active(),
                step == 0
            );
            assert_eq!(
                app.world().resource::<terra_bevy::PoliticalWorld>().drawing,
                step < 2
            );
        }
        assert!(
            app.world()
                .resource::<terra_bevy::PoliticalWorld>()
                .draft
                .is_empty()
        );
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .reset_all();
        app.update();
        assert!(!app.world().resource::<GameFlow>().blocks_input());
        assert!(!app.world().resource::<Time<Virtual>>().is_paused());
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Escape);
        app.update();
        assert_eq!(
            app.world().resource::<GameFlow>().phase,
            Phase::Paused(Page::Session)
        );
    }

    #[test]
    fn manual_exit_waits_for_real_save_acknowledgement() {
        let persistence = RegionalPersistence::default();
        let mut queue = SandboxQueue::default();
        queue.0.push_back(SandboxIntent::Save);
        assert!(
            checkpoint_finished(
                0,
                &persistence,
                &queue,
                "Saved world, factions, politics and clock to old.json"
            )
            .is_none()
        );
        queue.0.clear();
        assert!(
            checkpoint_finished(
                0,
                &persistence,
                &queue,
                "Waiting for the requested checkpoint…"
            )
            .is_none()
        );
        assert_eq!(
            checkpoint_finished(
                0,
                &persistence,
                &queue,
                "Saved world, factions, politics and clock to new.json"
            ),
            Some(Ok(()))
        );
        assert!(
            checkpoint_finished(0, &persistence, &queue, "Action rejected: disk full")
                .unwrap()
                .is_err()
        );
    }

    #[test]
    fn local_menu_pause_preserves_saved_clock_and_prior_virtual_pause() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_plugins(terra_bevy::TerraWorldPlugin::new(terra_bevy::WorldConfig {
                chunk_radius: 0,
                terrain_resolution: 9,
                live_weather: false,
                ..default()
            }))
            .init_resource::<GameFlow>()
            .init_resource::<Actions>()
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<crate::player_input::PlayerInput>()
            .add_systems(Update, keyboard_and_pause);
        let before = app.world().resource::<WorldRuntime>().clock;
        app.update();
        assert!(app.world().resource::<Time<Virtual>>().is_paused());
        assert_eq!(
            app.world().resource::<WorldRuntime>().clock.paused,
            before.paused
        );
        app.world_mut()
            .resource_mut::<GameFlow>()
            .change(Phase::Playing);
        app.update();
        assert!(!app.world().resource::<Time<Virtual>>().is_paused());
        app.world_mut().resource_mut::<Time<Virtual>>().pause();
        app.world_mut()
            .resource_mut::<GameFlow>()
            .change(Phase::Paused(Page::Session));
        app.update();
        app.world_mut()
            .resource_mut::<GameFlow>()
            .change(Phase::Playing);
        app.update();
        assert!(app.world().resource::<Time<Virtual>>().is_paused());
    }

    #[test]
    fn multiplayer_overlay_never_pauses_simulation_time() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_plugins(terra_bevy::TerraWorldPlugin::new(terra_bevy::WorldConfig {
                chunk_radius: 0,
                terrain_resolution: 9,
                live_weather: false,
                ..default()
            }))
            .init_resource::<GameFlow>()
            .init_resource::<Actions>()
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<crate::player_input::PlayerInput>()
            .add_systems(Update, keyboard_and_pause);
        app.world_mut()
            .resource_mut::<WorldRuntime>()
            .network_authority = true;
        app.update();
        assert!(!app.world().resource::<Time<Virtual>>().is_paused());
        assert!(app.world().resource::<GameFlow>().blocks_input());
    }

    #[test]
    fn published_but_uncertain_final_save_emits_error_exit_and_keeps_readable_progress() {
        let folder = std::env::temp_dir().join(format!(
            "terra-uncertain-exit-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&folder).unwrap();
        let config = terra_bevy::WorldConfig {
            chunk_radius: 0,
            terrain_resolution: 9,
            time_scale: 0.,
            live_weather: false,
            save_path: folder.join("world-export.json"),
            cache_dir: folder.join("cache"),
            ..default()
        };
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_plugins(WindowPlugin {
                primary_window: None,
                ..default()
            })
            .insert_resource(DemoOptions {
                world_profile_path: Some(folder.join("world.json")),
                ..default()
            })
            .insert_resource(ExitPublicationSync(|_| {
                Err(std::io::Error::other("injected final-save parent sync"))
            }))
            .add_plugins(terra_bevy::TerraWorldPlugin::new(config.clone()))
            .add_systems(
                Last,
                save_managed_json_on_exit
                    .in_set(bevy::app::OnAppExitSystems)
                    .in_set(terra_bevy::RegionPersistenceSet::FinalCheckpoint)
                    .after(bevy::window::ExitSystems),
            );
        app.world_mut().resource_mut::<SandboxRuntime>().dirty = true;
        app.update();
        assert!(
            app.should_exit().is_some_and(|exit| exit.is_error()),
            "visible new bytes cannot acknowledge a successful exit after sync failure"
        );
        assert!(app.world().resource::<SandboxRuntime>().dirty);
        let saved = terra_bevy::sandbox::LocalWorldSave::read(
            &config.save_path,
            &config,
            &app.world().resource::<SandboxRuntime>().catalog,
        )
        .unwrap();
        assert_eq!(
            saved.player_id,
            app.world().resource::<SandboxRuntime>().player_id
        );
        assert!(config.save_path.is_file());
        drop(app);
        std::fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn clean_portable_exit_restores_the_saved_character_clock_and_failure_is_non_destructive() {
        let folder = std::env::temp_dir().join(format!(
            "terra-flow-portable-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&folder).unwrap();
        let config = terra_bevy::WorldConfig {
            chunk_radius: 0,
            terrain_resolution: 9,
            time_scale: 0.,
            live_weather: false,
            save_path: folder.join("world-export.json"),
            cache_dir: folder.join("cache"),
            ..default()
        };
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_plugins(WindowPlugin {
                primary_window: None,
                ..default()
            })
            .insert_resource(DemoOptions {
                world_profile_path: Some(folder.join("world.json")),
                ..default()
            })
            .add_plugins(terra_bevy::TerraWorldPlugin::new(config.clone()))
            .add_systems(
                Last,
                save_managed_json_on_exit
                    .in_set(bevy::app::OnAppExitSystems)
                    .in_set(terra_bevy::RegionPersistenceSet::FinalCheckpoint)
                    .after(bevy::window::ExitSystems),
            );
        let character = terra_bevy::rules::CharacterProfile {
            name: "Saved driver".into(),
            body_style: 2,
            skin: 5,
            hair: 2,
            outfit: 6,
        };
        app.world_mut()
            .resource_mut::<SandboxRuntime>()
            .state
            .set_character_profile(1, character.clone())
            .unwrap();
        {
            let mut runtime = app.world_mut().resource_mut::<WorldRuntime>();
            runtime.clock.unix_seconds = 1_234_567_890.;
            runtime.clock.time_scale = 15.;
            runtime.clock.paused = true;
        }
        app.update();
        assert_eq!(app.should_exit(), Some(AppExit::Success));
        let original = std::fs::read(&config.save_path).unwrap();
        drop(app);
        let mut resumed = App::new();
        resumed
            .add_plugins(MinimalPlugins)
            .insert_resource(DemoOptions {
                world_profile_path: Some(folder.join("world.json")),
                ..default()
            })
            .add_plugins(terra_bevy::TerraWorldPlugin::new(config.clone()));
        crate::restore_managed_json(&mut resumed).unwrap();
        assert_eq!(
            resumed
                .world()
                .resource::<SandboxRuntime>()
                .restore_revision,
            1
        );
        assert_eq!(
            resumed
                .world()
                .resource::<SandboxRuntime>()
                .state
                .transport
                .profiles[&1],
            character
        );
        assert_eq!(
            resumed
                .world()
                .resource::<WorldRuntime>()
                .clock
                .unix_seconds,
            1_234_567_890.
        );
        assert_eq!(
            resumed.world().resource::<WorldRuntime>().clock.time_scale,
            15.
        );
        assert!(resumed.world().resource::<WorldRuntime>().clock.paused);
        assert_eq!(std::fs::read(&config.save_path).unwrap(), original);
        std::fs::write(&config.save_path, b"corrupt progress").unwrap();
        assert!(crate::restore_managed_json(&mut resumed).is_err());
        assert_eq!(
            resumed
                .world()
                .resource::<SandboxRuntime>()
                .restore_revision,
            1
        );
        assert_eq!(
            resumed
                .world()
                .resource::<SandboxRuntime>()
                .state
                .transport
                .profiles[&1],
            character
        );
        assert_eq!(
            std::fs::read(&config.save_path).unwrap(),
            b"corrupt progress"
        );
        drop(resumed);
        std::fs::remove_dir_all(folder).unwrap();
    }
}
