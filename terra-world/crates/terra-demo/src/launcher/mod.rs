//! Native world selection and setup, before the simulation process is started.
pub(crate) mod model;
mod preview;
mod recovery;
mod ui;

use bevy::{app::AppExit, input_focus::tab_navigation::TabNavigationPlugin, prelude::*};
use model::{Draft, Preferences, RecentWorld, WorldProfile};
use std::{
    collections::VecDeque,
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{Arc, Mutex},
    thread::JoinHandle,
};

pub(crate) const MENU_RETURN_PREFIX: &str = "TERRA_MENU_RETURN ";
pub(crate) const MENU_CONTROLS_PREFIX: &str = "TERRA_MENU_CONTROLS ";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Screen {
    Home,
    Worlds,
    RenameWorld,
    Backups,
    RecoveryReview,
    Character,
    World,
    Review,
    Join,
    Settings,
    Controls,
    Launching,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Action {
    Home,
    NewWorld,
    Worlds,
    Character,
    World,
    Review,
    Create,
    Continue(usize, u64),
    BackupWorld(usize, u64),
    RenameWorld(usize, u64),
    SaveWorldName(u64),
    Backups,
    RefreshBackups,
    ReviewBackup(usize, u64),
    InspectBackupPath,
    RestoreBackup(u64),
    Join,
    Connect,
    Settings,
    Controls,
    Quit,
    Body,
    Skin,
    Hair,
    Outfit,
    Rotate,
    Preset,
    Radius,
    TimeScale,
    StartHour,
    DayNight,
    Seasons,
    Weather,
    Vegetation,
    Storage,
    Tls,
    InsecureRemote,
    Quality,
    Sky,
    PlaytestReports,
    RefreshReport,
    SaveSettings,
    Refresh,
    InputSetting(crate::player_input::SettingAction),
}
#[derive(Resource, Default)]
struct Actions(VecDeque<Action>);

#[derive(Resource)]
struct MenuState {
    project_root: PathBuf,
    user_data: PathBuf,
    screen: Screen,
    draft: Draft,
    playtest_reports: bool,
    report_summary: String,
    recent: Vec<RecentWorld>,
    backups: Vec<recovery::Backup>,
    completed_world: Option<PathBuf>,
    completed_backup: Option<PathBuf>,
    listing_revision: u64,
    recovery_context: u64,
    review_revision: u64,
    recovery_review: Option<recovery::BackupReview>,
    rename_selection: Option<RecentWorld>,
    world_name_draft: String,
    backup_path: String,
    recovery_progress: Option<String>,
    notice: Option<String>,
    warning: Option<String>,
    error: Option<String>,
    dirty: bool,
    focus: Option<Action>,
    preview_angle: f32,
    output: Arc<Mutex<Option<Vec<String>>>>,
    exit_frames: Option<u64>,
    screenshot: Option<String>,
    frames: u64,
}

enum RecoveryOutcome {
    Created(recovery::Backup),
    Reviewed(Box<recovery::BackupReview>),
    Restored(recovery::RestoredWorld),
}

#[derive(Resource, Default)]
struct RecoveryWorker {
    job: Option<JoinHandle<Result<RecoveryOutcome, String>>>,
    context: u64,
}

fn report_summary(user_data: &Path) -> String {
    match crate::telemetry_capture::read_summary(&crate::telemetry_capture::menu_report_path(user_data)) {
        Ok(Some(summary)) => summary.describe(),
        Ok(None) => "No local playtest report yet. With diagnostics On, capture starts as the world opens, then repeats every 10 real seconds.".into(),
        Err(error) => format!("The existing report could not be read: {error}. It has been left unchanged."),
    }
}

fn append_playtest_arguments(arguments: &mut Vec<String>, enabled: bool, user_data: &Path) {
    if enabled {
        arguments.extend([
            "--telemetry".into(),
            crate::telemetry_capture::menu_report_path(user_data)
                .to_string_lossy()
                .into_owned(),
            "--telemetry-interval-seconds".into(),
            crate::telemetry_capture::MENU_INTERVAL_SECONDS.to_string(),
        ]);
    }
}

fn refresh_worlds(menu: &mut MenuState) {
    let (mut recent, warnings) = model::recent_worlds(&menu.user_data);
    menu.warning = warnings.first().cloned();
    if let Some(path) = menu.completed_world.clone() {
        match WorldProfile::read(&path) {
            Ok(profile) => {
                recent.retain(|world| world.path != path);
                recent.insert(
                    0,
                    RecentWorld {
                        path,
                        has_checkpoint_file: model::has_checkpoint_file(&profile),
                        profile,
                    },
                );
                recent.truncate(model::MAX_RECENT_WORLDS);
            }
            Err(error) => {
                menu.completed_world = None;
                menu.warning = Some(format!(
                    "The selected world is no longer available: {error}"
                ));
            }
        }
    }
    menu.recent = recent;
    menu.listing_revision = menu.listing_revision.wrapping_add(1);
}

fn refresh_backups(menu: &mut MenuState) {
    let (mut backups, warnings) = recovery::recent_backups(&menu.user_data);
    menu.warning = warnings.first().cloned();
    if let Some(path) = menu.completed_backup.clone() {
        match recovery::read_backup(&path) {
            Ok(backup) => {
                backups.retain(|candidate| candidate.directory != backup.directory);
                backups.insert(0, backup);
                backups.truncate(recovery::MAX_RECENT_BACKUPS);
            }
            Err(error) => {
                menu.completed_backup = None;
                menu.warning = Some(format!("The new backup is no longer available: {error}"));
            }
        }
    }
    menu.backups = backups;
    menu.listing_revision = menu.listing_revision.wrapping_add(1);
}

fn start_recovery(
    menu: &mut MenuState,
    worker: &mut RecoveryWorker,
    progress: String,
    operation: impl FnOnce() -> Result<RecoveryOutcome, String> + Send + 'static,
) -> Result<(), String> {
    if worker.job.is_some() {
        return Err("A recovery operation is already running. Wait for its result.".into());
    }
    let job = std::thread::Builder::new()
        .name("terra-menu-recovery".into())
        .spawn(operation)
        .map_err(|error| format!("Could not start recovery: {error}"))?;
    worker.context = menu.recovery_context;
    worker.job = Some(job);
    menu.recovery_progress = Some(progress);
    menu.notice = None;
    Ok(())
}

fn poll_recovery(mut menu: ResMut<MenuState>, mut worker: ResMut<RecoveryWorker>) {
    if !worker.job.as_ref().is_some_and(JoinHandle::is_finished) {
        return;
    }
    let result = worker.job.take().unwrap().join()
        .unwrap_or_else(|_| Err("Recovery stopped unexpectedly. No incomplete copy is offered as a world. Keep existing worlds and backups, then try again.".into()));
    menu.recovery_progress = None;
    menu.error = None;
    match result {
        Ok(RecoveryOutcome::Created(backup)) => {
            let ready = format!(
                "Backup ready · {} · checkpoint {}. Open Backups to review a recovery copy.",
                backup.record.title, backup.record.source_generation
            );
            menu.completed_backup = Some(backup.directory);
            refresh_backups(&mut menu);
            if menu.completed_backup.is_some() {
                menu.notice = Some(ready);
            } else {
                menu.error = Some("The backup operation completed, but its saved record could not be reopened. Refresh Backups and check the reported folder error.".into());
            }
        }
        Ok(RecoveryOutcome::Reviewed(review)) if worker.context == menu.recovery_context => {
            menu.review_revision = menu.review_revision.wrapping_add(1);
            menu.recovery_review = Some(*review);
            menu.screen = Screen::RecoveryReview;
            menu.focus = Some(Action::Backups);
        }
        Ok(RecoveryOutcome::Reviewed(_)) => {
            menu.notice = Some("The backup passed verification. Select it again when you are ready to review a restore.".into());
        }
        Ok(RecoveryOutcome::Restored(restored)) => {
            menu.completed_world = Some(restored.profile_path.clone());
            refresh_worlds(&mut menu);
            let selected = menu
                .recent
                .iter()
                .position(|world| world.path == restored.profile_path);
            if selected.is_some() {
                menu.notice = Some(format!(
                    "Recovered · {} · checkpoint {}. Your separate world is ready in Your worlds; choose Open when ready.",
                    restored.title, restored.checkpoint
                ));
            } else {
                menu.error = Some(format!(
                    "The restore completed, but its profile could not be reopened at {}. Refresh Your worlds and check the reported folder error.",
                    restored.profile_path.display()
                ));
            }
            if worker.context == menu.recovery_context {
                menu.screen = Screen::Worlds;
                menu.focus = selected.map(|index| Action::Continue(index, menu.listing_revision));
            }
        }
        Err(error) => menu.error = Some(error),
    }
    menu.dirty = true;
}

fn close_menu(
    mut requests: MessageReader<bevy::window::WindowCloseRequested>,
    mut menu: ResMut<MenuState>,
    mut exit: MessageWriter<AppExit>,
) {
    if requests.read().count() == 0 {
        return;
    }
    if menu.recovery_progress.is_some() {
        menu.error =
            Some("Wait for the current recovery operation to finish before closing Terra.".into());
        menu.dirty = true;
    } else {
        exit.write(AppExit::Success);
    }
}

pub(crate) fn run(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let project_root = std::env::current_dir()?.canonicalize()?;
    let mut user_data = project_root.join("saves");
    let mut start = Screen::Home;
    let mut notice = None;
    let mut exit_frames = None;
    let mut screenshot = None;
    let mut controls_override = None;
    let mut i = 0;
    while i < args.len() {
        let next = |at: usize| {
            args.get(at + 1)
                .ok_or_else(|| format!("{} requires a value", args[at]))
        };
        match args[i].as_str() {
            "--menu" => {}
            "--controls-json" => {
                controls_override =
                    Some(crate::player_input::InputSettings::from_handoff(next(i)?)?);
                i += 1;
            }
            "--user-data" => {
                user_data = model::absolute_path(Path::new(next(i)?))?;
                i += 1;
            }
            "--menu-error" => {
                notice = Some(next(i)?.chars().take(600).collect());
                i += 1;
            }
            "--menu-screen" => {
                start = match next(i)?.as_str() {
                    "home" => Screen::Home,
                    "worlds" => Screen::Worlds,
                    "backups" => Screen::Backups,
                    "character" => Screen::Character,
                    "world" => Screen::World,
                    "review" => Screen::Review,
                    "join" => Screen::Join,
                    "settings" => Screen::Settings,
                    "controls" => Screen::Controls,
                    _ => return Err("unknown menu screen".into()),
                };
                i += 1;
            }
            "--frames" => {
                let count = next(i)?.parse::<u64>()?;
                if count < 30 {
                    return Err("menu --frames must be at least30".into());
                }
                exit_frames = Some(count);
                i += 1;
            }
            "--screenshot" => {
                screenshot = Some(next(i)?.clone());
                i += 1;
            }
            flag => return Err(format!("unsupported menu option: {flag}").into()),
        }
        i += 1;
    }
    if let Some(path) = screenshot.as_ref().map(PathBuf::from) {
        model::validate_menu_screenshot(&path)?;
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent)?;
        }
    }
    let (preferences, preferences_warning) = Preferences::read(&user_data);
    let mut draft = Draft {
        quality: preferences.quality,
        ..default()
    };
    let connection_warning = draft.restore_connection(&user_data);
    let (recent, warnings) = model::recent_worlds(&user_data);
    let (backups, backup_warnings) = recovery::recent_backups(&user_data);
    let output = Arc::new(Mutex::new(None));
    let mut app = App::new();
    app.insert_resource(ClearColor(Color::srgb(0.06, 0.11, 0.10)))
        .insert_resource(MenuState {
            project_root,
            user_data: user_data.clone(),
            screen: start,
            draft,
            playtest_reports: preferences.playtest_reports,
            report_summary: report_summary(&user_data),
            recent,
            backups,
            completed_world: None,
            completed_backup: None,
            listing_revision: 0,
            recovery_context: 0,
            review_revision: 0,
            recovery_review: None,
            rename_selection: None,
            world_name_draft: String::new(),
            backup_path: String::new(),
            recovery_progress: None,
            notice: None,
            warning: preferences_warning
                .or(connection_warning)
                .or_else(|| warnings.first().cloned())
                .or_else(|| backup_warnings.first().cloned()),
            error: notice,
            dirty: true,
            focus: None,
            preview_angle: 0.1,
            output: output.clone(),
            exit_frames,
            screenshot,
            frames: 0,
        })
        .insert_resource(
            controls_override
                .unwrap_or_else(|| crate::player_input::InputSettings::read(&user_data)),
        )
        .init_resource::<Actions>()
        .init_resource::<RecoveryWorker>()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            close_when_requested: false,
            primary_window: Some(Window {
                title: "Terra World · Main menu".into(),
                resolution: (1440, 900).into(),
                resize_constraints: WindowResizeConstraints {
                    min_width: 1100.,
                    min_height: 700.,
                    ..default()
                },
                ..default()
            }),
            ..default()
        }))
        .add_plugins((TabNavigationPlugin, crate::player_input::PlayerInputPlugin))
        .add_observer(ui::complete_space_input)
        .add_systems(Startup, preview::setup)
        .add_systems(
            Update,
            (
                ui::sync_fields,
                poll_recovery,
                close_menu,
                process_actions.after(crate::accessibility::ControllerNavigation),
                ui::rebuild,
                ui::style,
                preview::update,
                verification,
            )
                .chain(),
        )
        .add_systems(
            PostUpdate,
            crate::flow::scroll_focus_into_view.after(bevy::ui::UiSystems::Layout),
        );
    app.world_mut()
        .resource_mut::<Assets<Font>>()
        .insert(
            bevy::asset::AssetId::default(),
            Font::from_bytes(include_bytes!("../../assets/DejaVuSans.ttf").to_vec()),
        )
        .expect("default font ID");
    let result = app.run();
    drop(app);
    if result.is_error() {
        return Err("native menu failed".into());
    }
    let launch = output.lock().map_err(|_| "menu result lock failed")?.take();
    if let Some(arguments) = launch {
        launch_game(arguments, &user_data)?;
    }
    Ok(())
}

fn launch_game(
    mut arguments: Vec<String>,
    user_data: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let controls = arguments
        .windows(2)
        .find(|pair| pair[0] == "--controls-json")
        .and_then(|pair| crate::player_input::InputSettings::from_handoff(&pair[1]).ok());
    // A fresh process owns a fresh native event loop. No shell evaluates fields.
    // Drain stderr continuously into a bounded tail so long sessions cannot fill
    // a pipe or retain an unbounded log in the menu supervisor.
    arguments.push("--menu-supervised".into());
    let mut child = Command::new(std::env::current_exe()?)
        .args(arguments)
        .stderr(Stdio::piped())
        .spawn()?;
    let mut stderr = child.stderr.take().ok_or("game stderr unavailable")?;
    let reader = std::thread::spawn(move || {
        let mut tail = VecDeque::new();
        let mut chunk = [0u8; 4096];
        loop {
            match stderr.read(&mut chunk) {
                Ok(0) => break,
                Ok(count) => {
                    tail.extend(&chunk[..count]);
                    while tail.len() > 8192 {
                        tail.pop_front();
                    }
                }
                Err(_) => break,
            }
        }
        String::from_utf8_lossy(&tail.into_iter().collect::<Vec<_>>()).into_owned()
    });
    let status = child.wait()?;
    let tail = reader.join().unwrap_or_default();
    if !status.success() {
        let detail = failed_session_notice(&tail);
        let controls = menu_return_controls(&tail).or(controls);
        spawn_menu(user_data, Some(&detail), controls.as_ref())?;
    } else if let Some(notice) = menu_return_notice(&tail) {
        let controls = menu_return_controls(&tail).or(controls);
        spawn_menu(user_data, notice.as_deref(), controls.as_ref())?;
    }
    Ok(())
}

fn failed_session_notice(tail: &str) -> String {
    let diagnostic = tail.lines().rev().find_map(|line| {
        line.strip_prefix(crate::telemetry_capture::FINAL_ERROR_PREFIX)
            .and_then(|payload| serde_json::from_str::<String>(payload).ok())
    });
    let world = tail
        .lines()
        .rev()
        .find(|line| {
            line.starts_with("Final regional checkpoint failed:")
                || line.contains("Terra World: final portable save failed:")
                || line.contains("Terra World: final session handoff could not be retained:")
        })
        .or_else(|| {
            tail.lines()
                .rev()
                .find(|line| line.contains("Terra World:"))
        });
    let bounded = |text: &str, limit: usize| -> String {
        text.chars()
            .filter(|c| !c.is_control() || *c == '\n')
            .take(limit)
            .collect()
    };
    let detail = match (world, diagnostic) {
        (Some(world), Some(diagnostic)) => format!("{}\nDiagnostics: {}", bounded(world, 280), bounded(&diagnostic, 280)),
        (Some(world), None) => world.into(),
        (None, Some(diagnostic)) => format!("Diagnostics: {diagnostic}\nReport export failed. Check the world's saved checkpoint in Your worlds; this message alone does not establish its save outcome."),
        (None, None) => "The world could not start or finish safely. Check its source configuration, storage permissions and recovery status.".into(),
    };
    detail
        .chars()
        .filter(|c| !c.is_control() || *c == '\n')
        .take(600)
        .collect()
}

fn menu_return_notice(tail: &str) -> Option<Option<String>> {
    tail.lines().rev().find_map(|line| {
        line.strip_prefix(MENU_RETURN_PREFIX)
            .and_then(|payload| serde_json::from_str::<Option<String>>(payload).ok())
    })
}

fn menu_return_controls(tail: &str) -> Option<crate::player_input::InputSettings> {
    tail.lines().rev().find_map(|line| {
        line.strip_prefix(MENU_CONTROLS_PREFIX)
            .and_then(|payload| crate::player_input::InputSettings::from_handoff(payload).ok())
    })
}

pub(crate) fn spawn_menu(
    user_data: &Path,
    notice: Option<&str>,
    controls: Option<&crate::player_input::InputSettings>,
) -> Result<(), String> {
    let mut command = Command::new(std::env::current_exe().map_err(|e| e.to_string())?);
    command.arg("--menu").arg("--user-data").arg(user_data);
    if let Some(controls) = controls {
        command.arg("--controls-json").arg(controls.to_handoff()?);
    }
    if let Some(notice) = notice {
        command
            .arg("--menu-error")
            .arg(notice.chars().take(600).collect::<String>());
    }
    // The old world/event loop is already gone. Replacing this process image
    // gives winit a fresh loop while retaining the caller's supervised process
    // lifetime across repeated world/menu transitions. It also avoids building
    // a chain of idle supervisors or leaving an orphaned menu behind.
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        Err(format!(
            "Could not return to the main menu: {}",
            command.exec()
        ))
    }
    #[cfg(not(unix))]
    {
        command
            .spawn()
            .map_err(|e| format!("Could not return to the main menu: {e}"))?;
        Ok(())
    }
}

fn process_actions(
    mut menu: ResMut<MenuState>,
    mut recovery_worker: ResMut<RecoveryWorker>,
    mut actions: ResMut<Actions>,
    keys: Res<ButtonInput<KeyCode>>,
    mut exit: MessageWriter<AppExit>,
    mut input_settings: ResMut<crate::player_input::InputSettings>,
    input: Res<crate::player_input::PlayerInput>,
    keyboard: Res<crate::accessibility::ControllerKeyboard>,
) {
    if input_settings.is_changed() {
        menu.dirty = true;
    }
    if menu.screen != Screen::Controls {
        input_settings.cancel_capture();
    }
    if (keys.just_pressed(KeyCode::Escape) || input.menu_back)
        && !input.suppress
        && !keyboard.is_open()
        && !keyboard.consumed
    {
        actions.0.push_back(match menu.screen {
            Screen::Character => Action::Home,
            Screen::World => Action::Character,
            Screen::Review => Action::World,
            Screen::Backups => Action::Worlds,
            Screen::RecoveryReview => Action::Backups,
            Screen::RenameWorld => Action::Worlds,
            Screen::Home => Action::Quit,
            Screen::Launching => return,
            _ => Action::Home,
        });
    }
    for _ in 0..16 {
        let Some(action) = actions.0.pop_front() else {
            break;
        };
        if menu.recovery_progress.is_some()
            && !matches!(
                action,
                Action::Home
                    | Action::Worlds
                    | Action::Refresh
                    | Action::Backups
                    | Action::RefreshBackups
            )
        {
            menu.error = Some("Wait for the current recovery operation to finish before starting another task or closing Terra.".into());
            menu.dirty = true;
            continue;
        }
        if matches!(
            action,
            Action::Home
                | Action::NewWorld
                | Action::Worlds
                | Action::Refresh
                | Action::Backups
                | Action::RefreshBackups
                | Action::Character
                | Action::World
                | Action::Review
                | Action::Join
                | Action::Settings
                | Action::Controls
                | Action::SaveSettings
        ) {
            menu.recovery_context = menu.recovery_context.wrapping_add(1);
            menu.recovery_review = None;
            menu.rename_selection = None;
        }
        menu.error = None;
        menu.focus = Some(action);
        let mut launch = None;
        let result: Result<(), String> = (|| {
            match action {
                Action::InputSetting(action) => input_settings.handle(action, &menu.user_data),
                Action::Home => menu.screen = Screen::Home,
                Action::NewWorld => {
                    menu.screen = Screen::Character;
                }
                Action::Worlds | Action::Refresh => {
                    refresh_worlds(&mut menu);
                    menu.screen = Screen::Worlds;
                }
                Action::Backups | Action::RefreshBackups => {
                    refresh_backups(&mut menu);
                    menu.screen = Screen::Backups;
                }
                Action::RenameWorld(index, revision) => {
                    let world = selected_world(&menu, index, revision)?;
                    menu.world_name_draft.clone_from(&world.profile.title);
                    menu.rename_selection = Some(world);
                    menu.review_revision = menu.review_revision.wrapping_add(1);
                    menu.screen = Screen::RenameWorld;
                    menu.notice = None;
                }
                Action::SaveWorldName(revision) => {
                    if menu.screen != Screen::RenameWorld || revision != menu.review_revision {
                        return Err("The name editor changed. Select the world again.".into());
                    }
                    let selected = menu
                        .rename_selection
                        .as_ref()
                        .ok_or("Select a world before changing its name.")?;
                    let path = selected.path.clone();
                    let title =
                        recovery::rename_world(selected, &menu.user_data, &menu.world_name_draft)?;
                    menu.rename_selection = None;
                    menu.completed_world = Some(path.clone());
                    refresh_worlds(&mut menu);
                    menu.screen = Screen::Worlds;
                    menu.focus = menu
                        .recent
                        .iter()
                        .position(|world| world.path == path)
                        .map(|index| Action::Continue(index, menu.listing_revision));
                    menu.notice = Some(format!(
                        "World renamed · {title}. Your progress stays in the same world."
                    ));
                }
                Action::BackupWorld(index, revision) => {
                    let world = selected_world(&menu, index, revision)?;
                    let selected = recovery::select_world(&world)?;
                    let user_data = menu.user_data.clone();
                    start_recovery(
                        &mut menu,
                        &mut recovery_worker,
                        format!(
                            "Creating backup · {}. Keep Terra open until the copy is verified.",
                            world.profile.title
                        ),
                        move || {
                            recovery::create_backup(selected, &user_data)
                                .map(RecoveryOutcome::Created)
                        },
                    )?;
                }
                Action::ReviewBackup(index, revision) => {
                    if revision != menu.listing_revision {
                        return Err("The backup list changed. Select the named copy again.".into());
                    }
                    let backup = menu
                        .backups
                        .get(index)
                        .cloned()
                        .ok_or("That backup is no longer listed. Refresh Backups.")?;
                    start_recovery(
                        &mut menu,
                        &mut recovery_worker,
                        format!(
                            "Verifying backup · {} · checkpoint {}…",
                            backup.record.title, backup.record.source_generation
                        ),
                        move || {
                            recovery::review_backup(backup)
                                .map(|review| RecoveryOutcome::Reviewed(Box::new(review)))
                        },
                    )?;
                }
                Action::InspectBackupPath => {
                    let path = menu.backup_path.trim();
                    if path.is_empty() || path.chars().count() > 1024 {
                        return Err("Enter the folder path of a completed Terra backup.".into());
                    }
                    let path = PathBuf::from(path);
                    start_recovery(
                        &mut menu,
                        &mut recovery_worker,
                        "Verifying the selected recovery folder…".into(),
                        move || {
                            recovery::read_backup(&path)
                                .and_then(recovery::review_backup)
                                .map(|review| RecoveryOutcome::Reviewed(Box::new(review)))
                        },
                    )?;
                }
                Action::RestoreBackup(revision) => {
                    if menu.screen != Screen::RecoveryReview || revision != menu.review_revision {
                        return Err(
                            "The recovery review changed. Select and verify the backup again."
                                .into(),
                        );
                    }
                    let title = &menu
                        .recovery_review
                        .as_ref()
                        .ok_or("Select and verify a backup before restoring it.")?
                        .restored_title;
                    model::validate_name(title, 48, "Recovered world name")?;
                    let review = menu
                        .recovery_review
                        .take()
                        .ok_or("Select and verify a backup before restoring it.")?;
                    let user_data = menu.user_data.clone();
                    let progress = format!(
                        "Recovering {} · from {} · checkpoint {}…",
                        review.restored_title.trim(),
                        review.backup.record.title,
                        review.verified.source_generation
                    );
                    menu.screen = Screen::Backups;
                    start_recovery(&mut menu, &mut recovery_worker, progress, move || {
                        recovery::restore_reviewed(review, &user_data)
                            .map(RecoveryOutcome::Restored)
                    })?;
                }
                Action::Character => menu.screen = Screen::Character,
                Action::World => {
                    model::validate_character(&menu.draft.character)?;
                    menu.screen = Screen::World;
                }
                Action::Review => {
                    menu.draft.config(&menu.project_root)?;
                    menu.screen = Screen::Review;
                }
                Action::Create => {
                    let path =
                        model::create_world(&menu.draft, &menu.project_root, &menu.user_data)?;
                    launch = Some(vec![
                        "--play".into(),
                        "--world-profile".into(),
                        path.to_string_lossy().into_owned(),
                        "--user-data".into(),
                        menu.user_data.to_string_lossy().into_owned(),
                    ]);
                }
                Action::Continue(index, revision) => {
                    let recent = selected_world(&menu, index, revision)?;
                    let current = WorldProfile::read(&recent.path)?;
                    if serde_json::to_value(&current).map_err(|e| e.to_string())?
                        != serde_json::to_value(&recent.profile).map_err(|e| e.to_string())?
                    {
                        return Err(
                            "The named world changed. Refresh Your worlds before opening it."
                                .into(),
                        );
                    }
                    launch = Some(vec![
                        "--play".into(),
                        "--world-profile".into(),
                        recent.path.to_string_lossy().into_owned(),
                        "--user-data".into(),
                        menu.user_data.to_string_lossy().into_owned(),
                    ]);
                }
                Action::Join => menu.screen = Screen::Join,
                Action::Connect => {
                    let arguments = menu.draft.join_arguments(&menu.user_data)?;
                    menu.draft.save_connection(&menu.user_data)?;
                    launch = Some(arguments);
                }
                Action::Settings => {
                    menu.report_summary = report_summary(&menu.user_data);
                    menu.screen = Screen::Settings;
                }
                Action::RefreshReport => menu.report_summary = report_summary(&menu.user_data),
                Action::PlaytestReports => menu.playtest_reports = !menu.playtest_reports,
                Action::Controls => menu.screen = Screen::Controls,
                Action::Quit => {
                    exit.write(AppExit::Success);
                }
                Action::Body => {
                    menu.draft.character.body_style = (menu.draft.character.body_style + 1) % 3
                }
                Action::Skin => menu.draft.character.skin = (menu.draft.character.skin + 1) % 8,
                Action::Hair => menu.draft.character.hair = (menu.draft.character.hair + 1) % 8,
                Action::Outfit => {
                    menu.draft.character.outfit = (menu.draft.character.outfit + 1) % 8
                }
                Action::Rotate => menu.preview_angle += std::f32::consts::FRAC_PI_4,
                Action::Preset => menu.draft.preset = menu.draft.preset.next(),
                Action::Radius => {
                    menu.draft.radius = match menu.draft.radius {
                        1 => 2,
                        2 => 3,
                        3 => 5,
                        _ => 1,
                    }
                }
                Action::TimeScale => {
                    menu.draft.time_scale = match menu.draft.time_scale {
                        1 => 15,
                        15 => 60,
                        60 => 120,
                        _ => 1,
                    }
                }
                Action::StartHour => {
                    menu.draft.start_hour = match menu.draft.start_hour {
                        6 => 12,
                        12 => 18,
                        18 => 22,
                        _ => 6,
                    }
                }
                Action::DayNight => menu.draft.day_night = !menu.draft.day_night,
                Action::Seasons => menu.draft.seasons = !menu.draft.seasons,
                Action::Weather => menu.draft.weather = !menu.draft.weather,
                Action::Vegetation => menu.draft.vegetation = !menu.draft.vegetation,
                Action::Storage => menu.draft.regional_storage = !menu.draft.regional_storage,
                Action::Tls => menu.draft.tls = !menu.draft.tls,
                Action::InsecureRemote => {
                    menu.draft.allow_insecure_remote = !menu.draft.allow_insecure_remote
                }
                Action::Quality => menu.draft.quality.quality = menu.draft.quality.quality.next(),
                Action::Sky => {
                    menu.draft.quality.atmospheric_sky = !menu.draft.quality.atmospheric_sky
                }
                Action::SaveSettings => {
                    Preferences {
                        quality: menu.draft.quality,
                        playtest_reports: menu.playtest_reports,
                    }
                    .write(&menu.user_data)?;
                    menu.screen = Screen::Home;
                }
            }
            Ok(())
        })();
        if let Err(error) = result {
            menu.error = Some(error);
        } else if matches!(
            action,
            Action::Home
                | Action::NewWorld
                | Action::Worlds
                | Action::Refresh
                | Action::Backups
                | Action::RefreshBackups
                | Action::Character
                | Action::World
                | Action::Review
                | Action::Join
                | Action::Settings
                | Action::Controls
                | Action::SaveSettings
        ) {
            menu.focus = None;
        }
        if let Some(mut arguments) = launch {
            append_playtest_arguments(&mut arguments, menu.playtest_reports, &menu.user_data);
            match input_settings.to_handoff() {
                Ok(controls) => arguments.extend(["--controls-json".into(), controls]),
                Err(error) => {
                    menu.error = Some(error);
                    menu.dirty = true;
                    continue;
                }
            }
            if let Ok(mut result) = menu.output.lock() {
                *result = Some(arguments);
            }
            menu.screen = Screen::Launching;
            exit.write(AppExit::Success);
        }
        if menu.screen != Screen::Controls {
            input_settings.cancel_capture();
        }
        menu.dirty = true;
    }
}

fn selected_world(menu: &MenuState, index: usize, revision: u64) -> Result<RecentWorld, String> {
    if revision != menu.listing_revision {
        return Err("The world list changed. Select the named world again.".into());
    }
    menu.recent
        .get(index)
        .cloned()
        .ok_or_else(|| "That world is no longer listed. Refresh Your worlds.".into())
}

fn verification(
    mut commands: Commands,
    mut menu: ResMut<MenuState>,
    mut exit: MessageWriter<AppExit>,
) {
    menu.frames += 1;
    if menu.frames == 25 {
        if let Some(path) = menu.screenshot.clone() {
            commands
                .spawn(bevy::render::view::screenshot::Screenshot::primary_window())
                .observe(bevy::render::view::screenshot::save_to_disk(path));
        }
    }
    if menu.recovery_progress.is_none()
        && menu.exit_frames.is_some_and(|frames| menu.frames >= frames)
    {
        exit.write(AppExit::Success);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn diagnostic_handoff_is_opt_in_and_keeps_paths_as_single_arguments() {
        let directory = std::path::Path::new("playtest data");
        let original = vec!["--play".to_owned()];
        let mut arguments = original.clone();
        super::append_playtest_arguments(&mut arguments, false, directory);
        assert_eq!(arguments, original);
        super::append_playtest_arguments(&mut arguments, true, directory);
        let path = directory
            .join("diagnostics")
            .join("latest.json")
            .to_string_lossy()
            .into_owned();
        assert_eq!(
            arguments,
            vec![
                "--play",
                "--telemetry",
                &path,
                "--telemetry-interval-seconds",
                "10"
            ]
        );
    }

    #[test]
    fn diagnostic_failure_is_distinct_from_world_failure_and_malformed_markers() {
        let detail = super::failed_session_notice(
            "TERRA_DIAGNOSTICS_ERROR \"report destination is busy\"\n",
        );
        assert!(detail.contains("report destination is busy"));
        assert!(detail.contains("does not establish its save outcome"));
        let both = super::failed_session_notice(
            "Terra World: checkpoint failed\nTERRA_DIAGNOSTICS_ERROR \"export failed\"\n",
        );
        assert!(both.contains("checkpoint failed"));
        assert!(both.contains("Diagnostics: export failed"));
        let invalid =
            super::failed_session_notice("TERRA_DIAGNOSTICS_ERROR {\"unexpected\":true}\n");
        assert!(invalid.contains("could not start or finish safely"));
        assert!(!invalid.contains("unexpected"));
        let oversized = format!("Terra World: {}", "x".repeat(2000));
        assert_eq!(
            super::failed_session_notice(&oversized).chars().count(),
            600
        );
        let combined =
            format!("{oversized}\nTERRA_DIAGNOSTICS_ERROR \"retained diagnostic failure\"\n");
        let bounded = super::failed_session_notice(&combined);
        assert!(bounded.contains("Terra World:"));
        assert!(bounded.contains("retained diagnostic failure"));
        assert!(bounded.chars().count() <= 600);
        let storage = super::failed_session_notice(
            "Final regional checkpoint failed: close and reopen the world\nTerra World: native runtime or requested verification failed\n",
        );
        assert!(storage.contains("close and reopen the world"));
        assert!(!storage.contains("requested verification"));
    }

    #[test]
    fn menu_return_keeps_valid_session_controls_and_rejects_malformed_handoffs() {
        use crate::player_input::{Binding, InputSettings};
        let mut settings = InputSettings::default();
        settings
            .bind(Binding::Forward, bevy::prelude::KeyCode::ArrowUp)
            .unwrap();
        let tail = format!(
            "{}{}\n{}null\n",
            super::MENU_CONTROLS_PREFIX,
            settings.to_handoff().unwrap(),
            super::MENU_RETURN_PREFIX
        );
        let returned = super::menu_return_controls(&tail).unwrap();
        assert_eq!(
            returned.key(Binding::Forward),
            bevy::prelude::KeyCode::ArrowUp
        );
        assert_eq!(super::menu_return_notice(&tail), Some(None));
        assert!(
            super::menu_return_controls("TERRA_MENU_CONTROLS {\"schema_version\":99}\n").is_none()
        );
    }
    #[test]
    fn menu_handoff_is_explicit_and_preserves_optional_failure_notice() {
        assert_eq!(super::menu_return_notice("ordinary game log"), None);
        assert_eq!(
            super::menu_return_notice("TERRA_MENU_RETURN null\n"),
            Some(None)
        );
        assert_eq!(
            super::menu_return_notice("TERRA_MENU_RETURN \"Settings failed: disk full\"\n"),
            Some(Some("Settings failed: disk full".into()))
        );
        assert_eq!(
            super::menu_return_notice("TERRA_MENU_RETURN invalid\n"),
            None
        );
    }
}
