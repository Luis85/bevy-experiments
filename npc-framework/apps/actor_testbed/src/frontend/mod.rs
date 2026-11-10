//! One native window from title to daily life, pause and a safely saved return.
//! This application layer owns lifecycle and files; gameplay enters as commands.
#[cfg(test)]
mod startup_tests;
mod ui;
use crate::island::session;
use bevy::input_focus::InputFocus;
use bevy::prelude::*;
use npc_framework::kernel::driver::NpcDriverSet;
use npc_framework::prelude::{SimClock, SimControl};
use npc_framework::view_helpers::ViewSystems;
use testbed_view::{FrontendGate, IslandMode, PlayerMenuRequest};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Screen {
    Title,
    New,
    Playing,
    Pause,
    Controls,
    Transition,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Destination {
    Title,
    Quit,
}

/// Renderer readiness belongs to presentation. A headless host has no such gate.
fn scene_ready(world: &World) -> bool {
    world
        .get_resource::<testbed_view::SceneReadiness>()
        .is_none_or(|scene| scene.ready && scene.error.is_none())
}

fn scene_preparation_message(world: &World) -> (&'static str, NoticeTone) {
    if world
        .get_resource::<testbed_view::SceneReadiness>()
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
    revision: u64,
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
            revision: 1,
        }
    }
}

#[derive(Component, Clone, Copy, Debug)]
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
    Inspect,
    Terraform,
}

pub(crate) fn install(app: &mut App) {
    testbed_view::game_ui::install(app);
    testbed_view::game_ui::install_panel_scroll(app);
    testbed_view::install_text_clipboard(app);
    app.init_resource::<FrontendState>()
        .insert_resource(testbed_view::PlayerExperience { enabled: true })
        .init_resource::<InputFocus>()
        .insert_resource(FrontendGate(true))
        .init_resource::<PlayerMenuRequest>()
        .add_systems(Startup, ui::spawn)
        .add_observer(ui::activate)
        .add_systems(
            Update,
            (ui::sync_name, ui::keyboard_navigation)
                .before(process)
                .after(ViewSystems::OverlayInput),
        )
        .add_systems(
            Update,
            (keys, process)
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
        .add_systems(Update, ui::refresh.after(ViewSystems::Present));
}

fn set_screen(world: &mut World, screen: Screen) {
    {
        let mut flow = world.resource_mut::<FrontendState>();
        flow.screen = screen;
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
            Screen::New | Screen::Controls => Some(Action::Back),
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

fn start_play(world: &mut World, new_name: Option<String>) -> Result<(), String> {
    start_in_mode(world, new_name, IslandMode::Character)
}

fn start_in_mode(
    world: &mut World,
    new_name: Option<String>,
    requested: IslandMode,
) -> Result<(), String> {
    if let Some(name) = new_name {
        session::new_game(world, &name)?;
    } else {
        session::continue_game(world)?;
    }
    // A saved key/approach is transient; committed work and its reservations are
    // physical state and continue exactly where the snapshot left them.
    testbed_view::prepare_player_suspend(world)?;
    testbed_view::clear_player_view_intent(world);
    let mut mode = if requested == IslandMode::Character && session::living_player(world) {
        IslandMode::Character
    } else {
        IslandMode::Inspector
    };
    let mut notice = String::new();
    let mut notice_tone = NoticeTone::Info;
    let entry = if requested == IslandMode::Inspector {
        testbed_view::watch_island_village(world)
    } else {
        testbed_view::request_island_mode(world, mode)
    };
    if let Err(error) = entry {
        mode = IslandMode::Inspector;
        testbed_view::request_island_mode(world, mode)?;
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
    testbed_view::request_island_mode(world, mode)?;
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
        if destination == Destination::Quit {
            world.write_message(AppExit::Success);
        } else {
            set_screen(world, Screen::Title);
        }
        return Ok(());
    }
    if world.resource::<FrontendState>().screen == Screen::Playing {
        pause(world);
    }
    let receipt = testbed_view::prepare_player_suspend(world)?;
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
        ) && !scene_ready(world)
        {
            continue;
        }
        let result = match action {
            Action::Continue if screen == Screen::Title => start_play(world, None),
            Action::ContinueInspector if screen == Screen::Title => {
                start_in_mode(world, None, IslandMode::Inspector)
            }
            Action::New if screen == Screen::Title => {
                set_screen(world, Screen::New);
                message(world, NoticeTone::Info, "");
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
            Action::Back => {
                let target = if screen == Screen::Controls {
                    world.resource::<FrontendState>().controls_back
                } else {
                    Screen::Title
                };
                set_screen(world, target);
                Ok(())
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
            Action::Inspect if screen == Screen::Pause => testbed_view::watch_island_village(world)
                .and_then(|()| resume(world, IslandMode::Inspector)),
            Action::Terraform if screen == Screen::Pause => resume(world, IslandMode::Editor),
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
    fn overseeing_new_and_saved_islands_defers_arrival_until_character_entry() {
        use npc_framework::{kernel::commands::CommandLog, prelude::SimIndex};
        use testbed_content::village::{VillageState, Villager, colony::AwaitingArrival};

        let dir = std::env::temp_dir().join(format!("npc-observer-entry-{}", std::process::id()));
        let path = dir.join("island.json");
        let host = |app: &mut App| {
            app.add_plugins(testbed_view::TestbedViewPlugin);
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
            testbed_view::request_island_mode(app.world_mut(), IslandMode::Character).unwrap();
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
            app.add_plugins(testbed_view::TestbedViewPlugin);
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
            app.add_plugins(testbed_view::TestbedViewPlugin);
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
        use npc_framework::kernel::hash::state_hash;
        use npc_framework::prelude::*;
        use testbed_content::village::{
            self, economy,
            homestead::{self, CottageBuild, HomesteadAct, HomesteadPlot},
        };
        let dir = std::env::temp_dir().join(format!("npc-flow-work-{}", std::process::id()));
        let path = dir.join("island.json");
        let host = |app: &mut App| {
            app.add_plugins(testbed_view::TestbedViewPlugin);
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
