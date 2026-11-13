//! One native window from title to daily life, pause and a safely saved return.
//! This application layer owns lifecycle and files; gameplay enters as commands.
mod export;
mod settings;
#[cfg(test)]
mod startup_tests;
mod ui;
mod worlds;
use crate::island::session;
use bevy::input_focus::InputFocus;
use bevy::prelude::*;
pub(crate) use export::ExportFolder;
#[cfg(test)]
pub(crate) use export::export as export_world;
use island_view::{FrontendGate, IslandMode, PlayerMenuRequest};
use npc_framework::kernel::driver::NpcDriverSet;
use npc_framework::prelude::{SimClock, SimControl};
use npc_framework::view_helpers::ViewSystems;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Screen {
    Title,
    New,
    Playing,
    Pause,
    Controls,
    Settings,
    Transition,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Destination {
    Title,
    Quit,
    /// Saved, then the New-game screen with the exported world chosen.
    NewGame,
}

/// Renderer readiness belongs to presentation. A headless host has no such gate.
fn scene_ready(world: &World) -> bool {
    world
        .get_resource::<island_view::SceneReadiness>()
        .is_none_or(|scene| scene.ready && scene.error.is_none())
}

fn scene_preparation_message(world: &World) -> (&'static str, NoticeTone) {
    if world
        .get_resource::<island_view::SceneReadiness>()
        .is_some_and(|scene| scene.error.is_some())
    {
        (
            "The island could not finish preparing. Quit and restart to try again.",
            NoticeTone::Error,
        )
    } else {
        (
            "Preparing the island… You can start once the landscape is ready.",
            NoticeTone::Info,
        )
    }
}
struct PendingExit {
    destination: Destination,
    receipt: Option<(u64, u32)>,
    started: std::time::Instant,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum NoticeTone {
    #[default]
    Info,
    Success,
    Error,
}

#[derive(Resource)]
pub(crate) struct FrontendState {
    screen: Screen,
    controls_back: Screen,
    requests: Vec<Action>,
    message: String,
    notice_tone: NoticeTone,
    resume_mode: IslandMode,
    resume_multiplier: u32,
    pending: Option<PendingExit>,
    /// The control to focus after a rebuild, so repeated settings steps keep
    /// their place for keyboard users.
    refocus: Option<Action>,
    revision: u64,
    settings_tab: SettingsTab,
    guide_page: u8,
    /// The player's speed while the played character sleeps faster.
    sleep_restore: Option<u32>,
}
impl Default for FrontendState {
    fn default() -> Self {
        Self {
            screen: Screen::Title,
            controls_back: Screen::Title,
            requests: Vec::new(),
            message: String::new(),
            notice_tone: NoticeTone::Info,
            resume_mode: IslandMode::Character,
            resume_multiplier: 1,
            pending: None,
            refocus: None,
            revision: 1,
            settings_tab: SettingsTab::default(),
            guide_page: 0,
            sleep_restore: None,
        }
    }
}

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Action {
    Continue,
    ContinueInspector,
    New,
    Begin,
    BeginInspector,
    Back,
    Controls,
    Pause,
    Resume,
    Save,
    Title,
    Quit,
    Settings,
    Adjust(settings::Change),
    /// A new island, entered in Shape.
    BeginShape,
    /// A tab of the Settings screen, or a page of the Guide.
    SettingsTab(SettingsTab),
    GuidePage(u8),
    /// A world in the New-game list, by its place in the list.
    ChooseWorld(u8),
}

/// The four tabs of the Settings screen.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum SettingsTab {
    #[default]
    Sound,
    Display,
    Controls,
    Accessibility,
}

impl SettingsTab {
    pub(crate) const ALL: [Self; 4] = [
        Self::Sound,
        Self::Display,
        Self::Controls,
        Self::Accessibility,
    ];
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Sound => "Sound",
            Self::Display => "Display",
            Self::Controls => "Controls",
            Self::Accessibility => "Accessibility",
        }
    }
}

pub(crate) fn install(app: &mut App) {
    settings::install(app);
    island_view::game_ui::install(app);
    island_view::game_ui::install_panel_scroll(app);
    island_view::install_text_clipboard(app);
    app.init_resource::<FrontendState>()
        .init_resource::<export::ExportState>()
        .init_resource::<worlds::WorldChoice>()
        .insert_resource(island_view::PlayerExperience { enabled: true })
        .init_resource::<InputFocus>()
        .insert_resource(FrontendGate(true))
        .init_resource::<PlayerMenuRequest>()
        .add_systems(Startup, ui::spawn)
        .add_observer(ui::activate)
        .add_observer(ui::slide)
        .add_systems(
            Update,
            (ui::sync_name, ui::keyboard_navigation)
                .before(process)
                .after(ViewSystems::OverlayInput),
        )
        .add_systems(
            Update,
            (keys, process, night)
                .chain()
                .after(ViewSystems::OverlayInput)
                .before(ViewSystems::Input)
                .before(NpcDriverSet)
                .before(crate::island::close_requested),
        )
        .configure_sets(
            Update,
            NpcDriverSet
                .after(ViewSystems::Input)
                .before(ViewSystems::Sync),
        )
        .add_systems(Update, ui::refresh.after(ViewSystems::Present))
        .add_systems(Update, export::process.after(ViewSystems::Present));
}

/// The sound of going from one screen to another: a menu opens over play,
/// closes back to it, and turns a page between menus.
pub(crate) fn screen_sound(
    from: Screen,
    to: Screen,
) -> Option<island_view::game_ui::sound::UiSound> {
    use island_view::game_ui::sound::UiSound;
    match (from == Screen::Playing, to == Screen::Playing) {
        _ if from == to => None,
        (true, false) => Some(UiSound::Open),
        (false, true) => Some(UiSound::Close),
        _ => Some(UiSound::Page),
    }
}

fn set_screen(world: &mut World, screen: Screen) {
    let from = world.resource::<FrontendState>().screen;
    if let Some(sound) = screen_sound(from, screen) {
        island_view::game_ui::sound::play(world, sound);
    }
    {
        let mut flow = world.resource_mut::<FrontendState>();
        flow.screen = screen;
        flow.refocus = None;
        flow.revision = flow.revision.wrapping_add(1);
    }
    world.resource_mut::<FrontendGate>().0 = screen != Screen::Playing;
    if screen != Screen::Playing {
        world.resource_mut::<SimControl>().pending_steps = 0;
    }
    world.resource_mut::<InputFocus>().clear();
    if let Some(mut mouse) = world.get_resource_mut::<ButtonInput<MouseButton>>() {
        mouse.reset_all();
    }
}
fn message(world: &mut World, tone: NoticeTone, text: impl Into<String>) {
    let mut flow = world.resource_mut::<FrontendState>();
    flow.message = text.into();
    flow.notice_tone = tone;
    flow.revision = flow.revision.wrapping_add(1);
}
fn keys(world: &mut World) {
    let requested = std::mem::take(&mut world.resource_mut::<PlayerMenuRequest>().0);
    if requested && world.resource::<FrontendState>().screen == Screen::Playing {
        world
            .resource_mut::<FrontendState>()
            .requests
            .push(Action::Pause);
    }
    let escape = world
        .get_resource::<ButtonInput<KeyCode>>()
        .is_some_and(|keys| keys.just_pressed(KeyCode::Escape));
    if escape {
        let screen = world.resource::<FrontendState>().screen;
        let action = match screen {
            Screen::New | Screen::Controls | Screen::Settings => Some(Action::Back),
            Screen::Pause => Some(Action::Resume),
            Screen::Playing
                if world.get_resource::<IslandMode>() != Some(&IslandMode::Character) =>
            {
                Some(Action::Pause)
            }
            _ => None,
        };
        if let Some(action) = action {
            world.resource_mut::<FrontendState>().requests.push(action);
            if let Some(mut keys) = world.get_resource_mut::<ButtonInput<KeyCode>>() {
                keys.reset(KeyCode::Escape);
            }
        }
    }
    // "Try again" on a failed save's or export's toast: the outcome is the
    // next toast.
    if island_view::take_island_toast_retry(world) {
        let offer = world.resource_mut::<export::ExportState>().offer.take();
        if world.resource::<export::ExportState>().retry {
            export::run(world);
        } else if let Some(path) = offer {
            // Saved first, through the ordinary exit; then the New-game
            // screen with this world chosen.
            world.resource_mut::<export::ExportState>().next_world = Some(path);
            if let Err(error) = begin_exit(world, Destination::NewGame) {
                message(world, NoticeTone::Error, error);
            }
        } else {
            let _ = crate::island::save(world);
        }
    }
    // The application keeps the window until its world is safely saved.
    let closing = world
        .get_resource::<Messages<bevy::window::WindowCloseRequested>>()
        .is_some_and(|requests| !requests.is_empty());
    if closing {
        world
            .resource_mut::<Messages<bevy::window::WindowCloseRequested>>()
            .clear();
        world
            .resource_mut::<FrontendState>()
            .requests
            .push(Action::Quit);
    }
}

/// The speed the host runs while the played character sleeps.
pub(crate) const SLEEP_SPEED: u32 = 30;

/// While the played character sleeps in Live, the host runs the clock
/// faster and gives the player's own speed back on waking. The simulation
/// skips nothing: it only runs more ticks per second. A speed the player
/// chooses during the night is kept.
fn night(world: &mut World) {
    let asleep = world.resource::<FrontendState>().screen == Screen::Playing
        && world.get_resource::<IslandMode>() == Some(&IslandMode::Character)
        && island_view::island_player_asleep(world);
    let restore = world.resource::<FrontendState>().sleep_restore;
    match (asleep, restore) {
        (true, None) => {
            let speed = world.resource::<SimControl>().multiplier;
            world.resource_mut::<FrontendState>().sleep_restore = Some(speed.max(1));
            world.resource_mut::<SimControl>().multiplier = SLEEP_SPEED;
        }
        (false, Some(speed)) => {
            world.resource_mut::<FrontendState>().sleep_restore = None;
            let mut control = world.resource_mut::<SimControl>();
            if control.multiplier == SLEEP_SPEED {
                control.multiplier = speed;
            }
        }
        _ => {}
    }
}

fn start_play(world: &mut World, new_name: Option<String>) -> Result<(), String> {
    start_in_mode(world, new_name, IslandMode::Character)
}

fn start_in_mode(
    world: &mut World,
    new_name: Option<String>,
    requested: IslandMode,
) -> Result<(), String> {
    if let Some(name) = new_name {
        let chosen = world
            .get_resource::<worlds::WorldChoice>()
            .and_then(worlds::WorldChoice::chosen_path);
        session::new_game_on(world, &name, chosen.as_deref())?;
    } else {
        session::continue_game(world)?;
    }
    // A saved key/approach is transient; committed work and its reservations are
    // physical state and continue exactly where the snapshot left them.
    island_view::prepare_player_suspend(world)?;
    island_view::clear_player_view_intent(world);
    let mut mode = if requested == IslandMode::Character && session::living_player(world) {
        IslandMode::Character
    } else {
        IslandMode::Inspector
    };
    let mut notice = String::new();
    let mut notice_tone = NoticeTone::Info;
    let entry = if requested == IslandMode::Inspector {
        island_view::watch_island_village(world)
    } else {
        island_view::request_island_mode(world, mode)
    };
    if let Err(error) = entry {
        mode = IslandMode::Inspector;
        island_view::request_island_mode(world, mode)?;
        notice_tone = NoticeTone::Error;
        notice =
            format!("You can inspect this island while character control is unavailable: {error}");
    }
    {
        let mut flow = world.resource_mut::<FrontendState>();
        flow.resume_mode = mode;
        flow.resume_multiplier = 1;
        flow.message = notice;
        flow.notice_tone = notice_tone;
    }
    let mut control = world.resource_mut::<SimControl>();
    control.paused = false;
    control.multiplier = 1;
    set_screen(world, Screen::Playing);
    Ok(())
}

fn pause(world: &mut World) {
    if world.resource::<FrontendState>().screen != Screen::Playing {
        return;
    }
    let mode = *world.resource::<IslandMode>();
    let speed = world.resource::<SimControl>().multiplier;
    {
        let mut flow = world.resource_mut::<FrontendState>();
        flow.resume_mode = mode;
        flow.resume_multiplier = speed.max(1);
        flow.message.clear();
        flow.notice_tone = NoticeTone::Info;
    }
    world.resource_mut::<SimControl>().paused = true;
    set_screen(world, Screen::Pause);
}
fn resume(world: &mut World, mode: IslandMode) -> Result<(), String> {
    island_view::request_island_mode(world, mode)?;
    let speed = world.resource::<FrontendState>().resume_multiplier.max(1);
    let mut control = world.resource_mut::<SimControl>();
    control.paused = false;
    control.multiplier = speed;
    set_screen(world, Screen::Playing);
    Ok(())
}

fn begin_exit(world: &mut World, destination: Destination) -> Result<(), String> {
    if world.resource::<FrontendState>().pending.is_some() {
        return Ok(());
    }
    let active = session::summary(world).active;
    if !active {
        match destination {
            Destination::Quit => {
                world.write_message(AppExit::Success);
            }
            Destination::Title => set_screen(world, Screen::Title),
            Destination::NewGame => open_new_game(world),
        }
        return Ok(());
    }
    if world.resource::<FrontendState>().screen == Screen::Playing {
        pause(world);
    }
    let receipt = island_view::prepare_player_suspend(world)?;
    world.resource_mut::<FrontendState>().pending = Some(PendingExit {
        destination,
        receipt,
        started: std::time::Instant::now(),
    });
    message(world, NoticeTone::Info, "Saving your island…");
    set_screen(world, Screen::Transition);
    // A bounded visible lifecycle transition, using the normal host driver.
    // No client step or authoritative outcome is manufactured to settle control.
    let mut control = world.resource_mut::<SimControl>();
    control.paused = receipt.is_none();
    control.multiplier = 1;
    Ok(())
}
fn finish_exit(world: &mut World) {
    let Some((destination, receipt, timed_out)) = world
        .resource::<FrontendState>()
        .pending
        .as_ref()
        .map(|pending| {
            (
                pending.destination,
                pending.receipt,
                pending.started.elapsed() >= std::time::Duration::from_secs(10),
            )
        })
    else {
        return;
    };
    let refused = receipt.and_then(|key| {
        world
            .get_resource::<npc_framework::kernel::commands::SkippedCommands>()
            .and_then(|skipped| {
                skipped
                    .entries
                    .iter()
                    .rev()
                    .find(|entry| (entry.tick, entry.seq) == key)
                    .map(|entry| entry.error.to_string())
            })
    });
    let ready = receipt.is_none_or(|key| {
        world.resource::<SimClock>().tick > key.0
            && world
                .get_resource::<npc_framework::kernel::commands::CommandQueue>()
                .is_none_or(|queue| !queue.pending.contains_key(&key))
    });
    if let Some(error) = refused {
        world.resource_mut::<FrontendState>().pending = None;
        world.resource_mut::<SimControl>().paused = true;
        set_screen(world, Screen::Pause);
        message(world, NoticeTone::Error, error);
        return;
    }
    if !ready && !timed_out {
        return;
    }
    world.resource_mut::<SimControl>().paused = true;
    world.resource_mut::<FrontendState>().pending = None;
    if !ready {
        set_screen(world, Screen::Pause);
        message(
            world,
            NoticeTone::Error,
            "Movement could not finish stopping. Your world is still open; resume or retry saving.",
        );
        return;
    }
    match session::save_for_title(world) {
        Ok(()) if destination == Destination::Quit => {
            world.write_message(AppExit::Success);
        }
        Ok(()) if destination == Destination::NewGame => {
            open_new_game(world);
            message(
                world,
                NoticeTone::Success,
                "Your island is saved and will be kept as a backup. Name your character and choose how to begin.",
            );
        }
        Ok(()) => {
            set_screen(world, Screen::Title);
            message(
                world,
                NoticeTone::Success,
                "Your island is saved. Continue whenever you are ready.",
            );
        }
        Err(error) => {
            set_screen(world, Screen::Pause);
            message(world, NoticeTone::Error, error);
        }
    }
}

/// The New-game screen, its world list read again; an exported world the
/// player asked for is chosen.
fn open_new_game(world: &mut World) {
    let prefer = world
        .resource_mut::<export::ExportState>()
        .next_world
        .take();
    let folder = world.get_resource::<ExportFolder>().map(|f| f.0.clone());
    let default = session::default_world(world).clone();
    world.resource_mut::<worlds::WorldChoice>().reload(
        &default,
        folder.as_deref(),
        prefer.as_deref(),
    );
    set_screen(world, Screen::New);
}

/// A final durability failure must restore an actionable shell, even if an
/// earlier save succeeded and the OS/file destination changed during shutdown.
pub(crate) fn exit_failed(world: &mut World, error: String) {
    if !world.contains_resource::<FrontendState>() {
        return;
    }
    world.resource_mut::<FrontendState>().pending = None;
    world.resource_mut::<SimControl>().paused = true;
    let screen = if session::summary(world).active {
        Screen::Pause
    } else {
        Screen::Title
    };
    set_screen(world, screen);
    message(world, NoticeTone::Error, error);
}

fn process(world: &mut World) {
    let actions = std::mem::take(&mut world.resource_mut::<FrontendState>().requests);
    for action in actions {
        let screen = world.resource::<FrontendState>().screen;
        if screen == Screen::Transition {
            continue;
        }
        if matches!(
            action,
            Action::Continue
                | Action::ContinueInspector
                | Action::New
                | Action::Begin
                | Action::BeginInspector
                | Action::BeginShape
        ) && !scene_ready(world)
        {
            continue;
        }
        let result = match action {
            Action::Continue if screen == Screen::Title => start_play(world, None).map(|()| {
                // Loading is said once, on the island, like a save.
                let player = session::summary(world).player;
                island_view::island_toast(
                    world,
                    island_view::ToastTone::Done,
                    format!("Island loaded. Welcome back, {player}."),
                    false,
                    false,
                );
            }),
            Action::ContinueInspector if screen == Screen::Title => {
                start_in_mode(world, None, IslandMode::Inspector)
            }
            Action::New if screen == Screen::Title => {
                open_new_game(world);
                message(world, NoticeTone::Info, "");
                Ok(())
            }
            Action::ChooseWorld(index) if screen == Screen::New => {
                {
                    let mut choice = world.resource_mut::<worlds::WorldChoice>();
                    let index = usize::from(index);
                    if choice
                        .entries
                        .get(index)
                        .is_some_and(|entry| entry.problem.is_none())
                    {
                        choice.chosen = index;
                    }
                }
                let mut flow = world.resource_mut::<FrontendState>();
                flow.refocus = Some(action);
                flow.revision = flow.revision.wrapping_add(1);
                Ok(())
            }
            Action::Begin if screen == Screen::New => {
                let name = ui::draft_name(world);
                start_play(world, Some(name))
            }
            Action::BeginInspector if screen == Screen::New => {
                let name = ui::draft_name(world);
                start_in_mode(world, Some(name), IslandMode::Inspector)
            }
            // Shape first: the island is overseen, with the tools in hand.
            Action::BeginShape if screen == Screen::New => {
                let name = ui::draft_name(world);
                start_in_mode(world, Some(name), IslandMode::Inspector).and_then(|()| {
                    island_view::request_island_mode(world, IslandMode::Editor)?;
                    world.resource_mut::<FrontendState>().resume_mode = IslandMode::Editor;
                    Ok(())
                })
            }
            Action::SettingsTab(tab) if screen == Screen::Settings => {
                let mut flow = world.resource_mut::<FrontendState>();
                flow.settings_tab = tab;
                flow.refocus = Some(action);
                flow.revision = flow.revision.wrapping_add(1);
                Ok(())
            }
            Action::GuidePage(page) if screen == Screen::Controls => {
                let mut flow = world.resource_mut::<FrontendState>();
                flow.guide_page = page;
                flow.refocus = Some(action);
                flow.revision = flow.revision.wrapping_add(1);
                Ok(())
            }
            Action::Back => {
                let target = if matches!(screen, Screen::Controls | Screen::Settings) {
                    world.resource::<FrontendState>().controls_back
                } else {
                    Screen::Title
                };
                set_screen(world, target);
                if screen == Screen::Settings {
                    message(world, NoticeTone::Info, "");
                }
                Ok(())
            }
            Action::Settings if matches!(screen, Screen::Title | Screen::Pause) => {
                world.resource_mut::<FrontendState>().controls_back = screen;
                set_screen(world, Screen::Settings);
                message(world, NoticeTone::Info, "");
                Ok(())
            }
            Action::Adjust(change) if screen == Screen::Settings => {
                let saved = settings::change(world, change);
                let mut flow = world.resource_mut::<FrontendState>();
                // A slider is filled in place: rebuilding the screen would
                // take the thumb from under the pointer.
                if !change.continuous() {
                    flow.refocus = Some(action);
                    flow.revision = flow.revision.wrapping_add(1);
                }
                if saved.is_ok() {
                    flow.message.clear();
                    flow.notice_tone = NoticeTone::Info;
                }
                saved
            }
            Action::Controls => {
                world.resource_mut::<FrontendState>().controls_back = screen;
                set_screen(world, Screen::Controls);
                Ok(())
            }
            Action::Pause => {
                pause(world);
                Ok(())
            }
            Action::Resume if screen == Screen::Pause => {
                let mode = world.resource::<FrontendState>().resume_mode;
                resume(world, mode)
            }
            Action::Save if screen == Screen::Pause => crate::island::save(world)
                .map(|()| message(world, NoticeTone::Success, "Your island is saved.")),
            Action::Title if screen == Screen::Pause => begin_exit(world, Destination::Title),
            Action::Quit => begin_exit(world, Destination::Quit),
            _ => Ok(()),
        };
        if let Err(error) = result {
            message(world, NoticeTone::Error, error);
        }
    }
    finish_exit(world);
}

#[cfg(test)]
mod tests {
    use super::*;
    use npc_framework::prelude::run_ticks;

    #[test]
    fn menus_open_over_play_close_back_to_it_and_turn_pages_between() {
        use island_view::game_ui::sound::UiSound;
        assert_eq!(
            screen_sound(Screen::Playing, Screen::Pause),
            Some(UiSound::Open)
        );
        assert_eq!(
            screen_sound(Screen::Pause, Screen::Playing),
            Some(UiSound::Close)
        );
        assert_eq!(
            screen_sound(Screen::Title, Screen::Settings),
            Some(UiSound::Page)
        );
        assert_eq!(screen_sound(Screen::Pause, Screen::Pause), None);
    }

    #[test]
    fn overseeing_new_and_saved_islands_defers_arrival_until_character_entry() {
        use island_content::village::{VillageState, Villager, colony::AwaitingArrival};
        use npc_framework::{kernel::commands::CommandLog, prelude::SimIndex};

        let dir = std::env::temp_dir().join(format!("npc-observer-entry-{}", std::process::id()));
        let path = dir.join("island.json");
        let host = |app: &mut App| {
            app.add_plugins(island_view::TestbedViewPlugin);
            install(app);
        };
        let mut app = session::open(&path, None, None, host).unwrap();
        start_in_mode(app.world_mut(), Some("Nora".into()), IslandMode::Inspector).unwrap();
        let actor = app.world().resource::<VillageState>().played;
        let entity = app.world().resource::<SimIndex>().get(actor).unwrap();
        assert_eq!(*app.world().resource::<IslandMode>(), IslandMode::Inspector);
        assert!(!app.world().resource::<SimControl>().is_stopped());
        run_ticks(app.world_mut(), 20);
        assert!(app.world().get::<AwaitingArrival>(entity).is_some());
        assert!(app.world().get::<Villager>(entity).is_none());
        assert!(
            app.world()
                .resource::<CommandLog>()
                .entries
                .iter()
                .all(|(_, _, command)| command.kind() != "JoinIsland")
        );

        begin_exit(app.world_mut(), Destination::Title).unwrap();
        finish_exit(app.world_mut());
        assert_eq!(
            app.world().resource::<FrontendState>().screen,
            Screen::Title
        );
        drop(app);

        let mut app = session::open(&path, None, None, host).unwrap();
        start_in_mode(app.world_mut(), None, IslandMode::Inspector).unwrap();
        run_ticks(app.world_mut(), 10);
        let entity = app.world().resource::<SimIndex>().get(actor).unwrap();
        assert!(app.world().get::<AwaitingArrival>(entity).is_some());
        for _ in 0..4 {
            island_view::request_island_mode(app.world_mut(), IslandMode::Character).unwrap();
            run_ticks(app.world_mut(), 2);
        }
        assert!(app.world().get::<AwaitingArrival>(entity).is_none());
        assert_eq!(app.world().get::<Villager>(entity).unwrap().name, "Nora");
        assert_eq!(
            app.world()
                .resource::<CommandLog>()
                .entries
                .iter()
                .filter(|(_, _, command)| command.kind() == "JoinIsland")
                .count(),
            1
        );
        assert_eq!(app.world().resource::<VillageState>().played, actor);
        drop(app);
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("world.lock"));
        let _ = std::fs::remove_dir(dir);
    }

    #[test]
    fn title_new_pause_and_saved_return_share_one_world_and_preserve_progress() {
        let dir = std::env::temp_dir().join(format!("npc-coherent-flow-{}", std::process::id()));
        let path = dir.join("island.json");
        let mut app = session::open(&path, None, None, |app| {
            app.add_plugins(island_view::TestbedViewPlugin);
            install(app);
        })
        .unwrap();
        assert_eq!(
            app.world().resource::<FrontendState>().screen,
            Screen::Title
        );
        assert!(!path.exists());
        start_play(app.world_mut(), Some("Robin".into())).unwrap();
        run_ticks(app.world_mut(), 3);
        pause(app.world_mut());
        assert_eq!(
            app.world().resource::<FrontendState>().screen,
            Screen::Pause
        );
        assert!(app.world().resource::<SimControl>().paused);
        let before = app.world().resource::<SimClock>().tick;
        begin_exit(app.world_mut(), Destination::Title).unwrap();
        finish_exit(app.world_mut());
        assert_eq!(
            app.world().resource::<FrontendState>().screen,
            Screen::Title
        );
        assert_eq!(snapshot_tick(&path), before);
        assert_eq!(
            app.world().resource::<FrontendState>().notice_tone,
            NoticeTone::Success
        );
        assert!(app.world().resource::<SimControl>().paused);
        drop(app);
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("world.lock"));
        let _ = std::fs::remove_dir(dir);
    }
    #[test]
    fn last_stage_save_failure_returns_to_pause_instead_of_trapping_shutdown() {
        let dir =
            std::env::temp_dir().join(format!("npc-flow-exit-failure-{}", std::process::id()));
        let path = dir.join("island.json");
        let mut app = session::open(&path, None, None, |app| {
            app.add_plugins(island_view::TestbedViewPlugin);
            install(app);
            app.init_resource::<Messages<AppExit>>();
        })
        .unwrap();
        start_play(app.world_mut(), Some("Robin".into())).unwrap();
        run_ticks(app.world_mut(), 3);
        begin_exit(app.world_mut(), Destination::Quit).unwrap();
        finish_exit(app.world_mut());
        assert!(!app.world().resource::<Messages<AppExit>>().is_empty());
        // Inject an OS destination failure after the first successful lifecycle save.
        std::fs::remove_file(&path).unwrap();
        std::fs::create_dir(&path).unwrap();
        crate::island::save_on_exit(app.world_mut());
        assert!(app.world().resource::<Messages<AppExit>>().is_empty());
        assert_eq!(
            app.world().resource::<FrontendState>().screen,
            Screen::Pause
        );
        assert!(
            app.world()
                .resource::<FrontendState>()
                .message
                .contains("Could not save")
        );
        assert!(app.world().resource::<FrontendState>().pending.is_none());
        assert_eq!(
            app.world().resource::<FrontendState>().notice_tone,
            NoticeTone::Error
        );
        drop(app);
        std::fs::remove_dir(&path).unwrap();
        let _ = std::fs::remove_file(path.with_extension("world.lock"));
        let _ = std::fs::remove_dir(dir);
    }

    #[test]
    fn cottage_work_and_crop_survive_title_save_reopen_and_continue() {
        use island_content::village::{
            self, economy,
            homestead::{self, CottageBuild, HomesteadAct, HomesteadPlot},
        };
        use npc_framework::kernel::hash::state_hash;
        use npc_framework::prelude::*;
        let dir = std::env::temp_dir().join(format!("npc-flow-work-{}", std::process::id()));
        let path = dir.join("island.json");
        let host = |app: &mut App| {
            app.add_plugins(island_view::TestbedViewPlugin);
            install(app);
        };
        let mut app = session::open(&path, None, None, host).unwrap();
        start_play(app.world_mut(), Some("Robin".into())).unwrap();
        run_ticks(app.world_mut(), 3);
        let actor_id = app.world().resource::<village::VillageState>().played;
        let actor = app.world().resource::<SimIndex>().get(actor_id).unwrap();
        let (plot_id, plot) = homestead::plots(app.world())
            .into_iter()
            .find(|(_, entity)| {
                homestead::definition(app.world(), *entity)
                    .unwrap()
                    .use_kind
                    == "housing"
            })
            .unwrap();
        // Fixture grants and relocation isolate lifecycle persistence, not travel
        // or material acquisition. All homestead actions remain recorded inputs.
        let center = *app.world().get::<SimPos>(plot).unwrap();
        app.world_mut()
            .entity_mut(actor)
            .insert(homestead::front_door(center))
            .remove::<MoveIntent>();
        for (kind, count) in [
            ("plank", 8),
            ("log", 4),
            (village::catalog::GRAIN, 1),
            (village::catalog::WATER, 1),
        ] {
            economy::grant(app.world_mut(), actor, Item::new(kind).stack(count));
        }
        for verb in ["claim_plot", "plant", "water_crop", "build_cottage"] {
            let point = if matches!(verb, "plant" | "water_crop") {
                homestead::bed_position(app.world(), plot, 0).unwrap()
            } else {
                homestead::front_door(center)
            };
            app.world_mut().entity_mut(actor).insert(point);
            HomesteadAct::new(actor_id, verb, plot_id)
                .check(app.world())
                .unwrap();
            submit(
                app.world_mut(),
                None,
                SimCommand::content(&HomesteadAct::new(actor_id, verb, plot_id)),
            )
            .unwrap();
            run_ticks(app.world_mut(), 1);
        }
        run_ticks(app.world_mut(), 37);
        let work = app.world().get::<CottageBuild>(actor).unwrap().clone();
        let crop = app.world().get::<HomesteadPlot>(plot).unwrap().beds[0]
            .clone()
            .unwrap();
        let hash = state_hash(app.world_mut());
        pause(app.world_mut());
        begin_exit(app.world_mut(), Destination::Title).unwrap();
        finish_exit(app.world_mut());
        assert_eq!(
            app.world().resource::<FrontendState>().screen,
            Screen::Title
        );
        assert_eq!(
            state_hash(app.world_mut()),
            hash,
            "saving must not cancel paid-for physical work"
        );
        drop(app);
        let mut app = session::open(&path, None, None, host).unwrap();
        assert_eq!(state_hash(app.world_mut()), hash);
        start_play(app.world_mut(), None).unwrap();
        assert_eq!(
            state_hash(app.world_mut()),
            hash,
            "Continue must keep native authority without releasing work"
        );
        let actor = app.world().resource::<SimIndex>().get(actor_id).unwrap();
        let plot = app.world().resource::<SimIndex>().get(plot_id).unwrap();
        assert_eq!(app.world().get::<CottageBuild>(actor), Some(&work));
        assert_eq!(
            app.world().get::<HomesteadPlot>(plot).unwrap().beds[0].as_ref(),
            Some(&crop)
        );
        for material in &work.materials {
            assert!(
                app.world()
                    .resource::<Reservations>()
                    .holds(material.item, actor_id)
            );
        }
        assert!(
            app.world()
                .resource::<Reservations>()
                .holds(plot_id, actor_id)
        );
        let remaining = work.ends - app.world().resource::<SimClock>().tick + 1;
        run_ticks(app.world_mut(), remaining);
        assert!(
            app.world()
                .get::<HomesteadPlot>(plot)
                .unwrap()
                .cottage_built
        );
        assert!(app.world().get::<CottageBuild>(actor).is_none());
        assert_eq!(economy::count(app.world(), actor, "plank"), 0);
        assert_eq!(economy::count(app.world(), actor, "log"), 0);
        drop(app);
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("world.lock"));
        let _ = std::fs::remove_dir(dir);
    }

    fn snapshot_tick(path: &std::path::Path) -> u64 {
        npc_framework::snapshot::read(path).unwrap().tick
    }
}
