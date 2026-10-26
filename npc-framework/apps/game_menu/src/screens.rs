//! The screens: one [`Screen`] state each, spawned on enter under a
//! [`DespawnOnExit`] root, with buttons carrying an [`Action`].
//!
//! Adding a screen: a [`Screen`] variant, its name in [`Screen::from_name`]
//! (for `--screen`), a spawn function registered in [`ScreensPlugin`] (the
//! explorer's are in [`crate::explorer`]), and [`Action`] variants for what
//! its buttons do, handled in [`apply_action`]. Launching goes through
//! [`Action::Launch`] with a [`crate::launch::Entry`].

use std::path::{Path, PathBuf};

use bevy::input_focus::{AutoFocus, InputFocus};
use bevy::prelude::*;
use bevy::text::EditableText;
use bevy::ui_widgets::{Activate, ScrollArea};
use testbed_content::explorer::names::NameKind;

use crate::detached::{self, DetachedPlugin, OpKind};
use crate::explorer::{self, ExplorerPlugin};
use crate::launch::{self, Entry, RunMode};
use crate::run::{self, BuildTools, Running};
use crate::ui::{self, ERROR, NOTE, OK, TEXT};
use crate::{AppSettings, Paths};

/// The menu's screens.
#[derive(States, Default, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Screen {
    #[default]
    Main,
    /// One button per embedded testbed scenario (but `explorer`).
    Testbed,
    /// The agents folder and the fog of war checkbox.
    Settings,
    /// The AI explorer's New / Resume choice.
    Explorer,
    /// Name a new agent and world.
    ExplorerNew,
    /// Pick an agent to resume.
    ExplorerResume,
    /// "Checking the save..." while `actor_testbed --check-save` runs.
    ExplorerCheck,
    /// The save does not fit this build (or cannot be read): the choice.
    ExplorerMismatch,
    /// A detached world: prompt, URL, tick, Open in window, Stop.
    ExplorerDetached,
    /// A game or a build is running.
    Running,
    /// The chosen game is not built.
    Missing,
    /// Quit stops every process the game runs ([`crate::quit`]).
    Quitting,
}

impl Screen {
    /// The `--screen` names.
    pub const NAMES: &'static str =
        "main, testbed, settings, explorer, explorer_new, explorer_resume";

    /// The screen `--screen name` opens.
    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "main" => Self::Main,
            "testbed" => Self::Testbed,
            "settings" => Self::Settings,
            "explorer" => Self::Explorer,
            "explorer_new" => Self::ExplorerNew,
            "explorer_resume" => Self::ExplorerResume,
            _ => return None,
        })
    }

    /// Where Back (and Escape) goes, or `None` (Main, Running).
    pub fn back(self) -> Option<Self> {
        match self {
            Self::Main | Self::Running | Self::Quitting => None,
            Self::Testbed | Self::Settings | Self::Explorer | Self::Missing => Some(Self::Main),
            Self::ExplorerNew | Self::ExplorerResume => Some(Self::Explorer),
            Self::ExplorerCheck | Self::ExplorerMismatch | Self::ExplorerDetached => {
                Some(Self::ExplorerResume)
            }
        }
    }
}

/// What a button does.
#[derive(Component, Clone, Debug)]
pub enum Action {
    Go(Screen),
    Launch(Entry),
    SaveSettings,
    /// The Settings screen's fog of war checkbox: flips and saves it.
    ToggleFog,
    /// The New screen's dice: another random name in that field.
    RandomName(NameKind),
    /// The New screen's Start in window / Start detached (checked again
    /// when pressed).
    StartNew(RunMode),
    /// Shows the agent's detached world.
    Watch(String),
    /// The agent's detached world into the window, on its port.
    OpenInWindow(String),
    /// Stops the agent's detached world (it saves first).
    StopDetached(String),
    /// The Detached screen's Copy: the prompt to the OS clipboard.
    CopyPrompt,
    Build,
    /// The Building screen's Cancel: kills the build, back to Missing.
    CancelBuild,
    Quit,
    /// The Quitting screen's **Quit anyway**: kills the worlds whose stop
    /// was refused (by pid; their saves stay at the last autosave), then
    /// quits.
    QuitAnyway,
    /// The Quitting screen's **Back**: the menu stays, the refused worlds
    /// keep running.
    QuitBack,
}

/// Main's one-line message (a game's non-zero exit, a failed start).
#[derive(Resource, Clone, Debug, Default, PartialEq, Eq)]
pub struct MainNotice(pub Option<String>);

impl MainNotice {
    pub fn error(text: String) -> Self {
        Self(Some(text))
    }
}

/// The Missing screen's game.
#[derive(Resource, Clone, Debug)]
pub struct Missing {
    pub stem: &'static str,
    pub entry: Entry,
    /// The command to run by hand.
    pub command: String,
    /// The last failed build or start.
    pub error: Option<String>,
}

impl Missing {
    pub fn new(stem: &'static str, entry: Entry, exe_dir: &Path) -> Self {
        Self {
            stem,
            entry,
            command: launch::command_line(&launch::build_args(stem, exe_dir)),
            error: None,
        }
    }
}

/// The Settings screen's status line.
#[derive(Resource, Clone, Debug, Default)]
struct SettingsStatus(Option<Result<String, String>>);

#[derive(Component)]
struct AgentsDirField;
#[derive(Component)]
struct SettingsStatusText;
/// The fog of war checkbox (a button whose label shows the box).
#[derive(Component)]
struct FogCheckbox;

/// The fog checkbox's label.
pub fn fog_checkbox_label(on: bool) -> &'static str {
    if on {
        "[x] Fog of war"
    } else {
        "[ ] Fog of war"
    }
}

/// The screens and their actions.
pub struct ScreensPlugin {
    /// The first screen (`--screen`).
    pub start: Screen,
}

impl Plugin for ScreensPlugin {
    fn build(&self, app: &mut App) {
        app.insert_state(self.start)
            .init_resource::<MainNotice>()
            .init_resource::<SettingsStatus>()
            .add_systems(OnEnter(Screen::Main), spawn_main)
            .add_systems(OnEnter(Screen::Testbed), spawn_testbed)
            .add_systems(OnEnter(Screen::Settings), spawn_settings)
            .add_systems(OnExit(Screen::Settings), |mut s: ResMut<SettingsStatus>| {
                s.0 = None;
            })
            .add_systems(OnEnter(Screen::Explorer), spawn_explorer)
            .add_plugins((ExplorerPlugin, DetachedPlugin, crate::quit::QuitPlugin))
            .add_systems(OnEnter(Screen::Running), spawn_running)
            .add_systems(OnEnter(Screen::Missing), spawn_missing)
            .add_systems(
                Update,
                (
                    run::poll_child.run_if(in_state(Screen::Running)),
                    escape_goes_back,
                    enter_saves.run_if(in_state(Screen::Settings)),
                    show_settings_status.run_if(in_state(Screen::Settings)),
                    show_fog_checkbox.run_if(in_state(Screen::Settings)),
                    ui::refresh_buttons,
                ),
            )
            .add_systems(
                Last,
                run::stop_build_on_exit.after(bevy::window::ExitSystems),
            );
    }
}

/// The observer every button gets: queues its [`Action`].
pub(crate) fn on_activate(ev: On<Activate>, actions: Query<&Action>, mut commands: Commands) {
    if let Ok(action) = actions.get(ev.entity).cloned() {
        commands.queue(move |world: &mut World| apply_action(world, action));
    }
}

/// Does what a button says.
pub fn apply_action(world: &mut World, action: Action) {
    match action {
        Action::Go(screen) => run::set_screen(world, screen),
        Action::Launch(entry) => {
            world.insert_resource(MainNotice::default());
            run::launch(world, entry);
        }
        Action::SaveSettings => save_settings(world),
        Action::ToggleFog => toggle_fog(world),
        Action::RandomName(kind) => explorer::reroll_name(world, kind),
        Action::StartNew(mode) => explorer::start_new(world, mode),
        Action::Watch(agent) => detached::watch(world, &agent),
        Action::OpenInWindow(agent) => {
            world.insert_resource(MainNotice::default());
            detached::begin_op(world, &agent, OpKind::Open);
        }
        Action::StopDetached(agent) => detached::begin_op(world, &agent, OpKind::Stop),
        Action::CopyPrompt => detached::copy_prompt(world),
        Action::Build => {
            let tools = world.resource::<BuildTools>().clone();
            let Some(missing) = world.get_resource::<Missing>().cloned() else {
                return;
            };
            if let (Some(cargo), Some(root)) = (tools.cargo, tools.root) {
                run::build(world, missing.entry, missing.stem, &cargo, &root);
            }
        }
        Action::CancelBuild => run::cancel_build(world),
        Action::Quit => quit(world),
        Action::QuitAnyway => crate::quit::quit_anyway(world),
        Action::QuitBack => crate::quit::back(world),
    }
}

/// Quit: stops every process the game started (detached worlds and
/// explorer windows save first), then exits ([`crate::quit`]); at once when
/// nothing runs.
pub fn quit(world: &mut World) {
    crate::quit::start(world, false);
}

/// The window's close button (the menu's window plugin does not close on
/// request): [`quit`].
pub fn close_quits(
    mut requests: MessageReader<bevy::window::WindowCloseRequested>,
    mut commands: Commands,
) {
    if requests.read().count() > 0 {
        commands.queue(quit);
    }
}

fn save_settings(world: &mut World) {
    let mut fields = world.query_filtered::<&EditableText, With<AgentsDirField>>();
    let Ok(text) = fields.single(world).map(|t| t.value().to_string()) else {
        return;
    };
    let file = world.resource::<Paths>().settings_file.clone();
    let fog = world.resource::<AppSettings>().0.fog_of_war;
    let status = match launch::save_settings(&text, fog, file.as_deref()) {
        Ok(settings) => {
            let msg = format!("saved: {}", settings.agents_dir.display());
            world.resource_mut::<AppSettings>().0 = settings;
            Ok(msg)
        }
        Err(e) => Err(e),
    };
    world.resource_mut::<SettingsStatus>().0 = Some(status);
}

/// Flips the fog of war setting and writes the settings file at once (the
/// agents folder as last saved); without a settings file it holds for this
/// session only.
fn toggle_fog(world: &mut World) {
    let settings = {
        let mut s = world.resource_mut::<AppSettings>();
        s.0.fog_of_war = !s.0.fog_of_war;
        s.0.clone()
    };
    let on = if settings.fog_of_war { "on" } else { "off" };
    let file = world.resource::<Paths>().settings_file.clone();
    let status = match file {
        Some(f) => settings
            .save(&f)
            .map(|()| format!("fog of war {on}: saved"))
            .map_err(|e| format!("fog of war {on}, but cannot write {}: {e}", f.display())),
        None => Ok(format!("fog of war {on} (not saved: no settings folder)")),
    };
    world.resource_mut::<SettingsStatus>().0 = Some(status);
}

/// Keeps the checkbox's label in step with the setting.
fn show_fog_checkbox(
    settings: Res<AppSettings>,
    boxes: Query<&Children, With<FogCheckbox>>,
    mut texts: Query<&mut Text>,
) {
    let want = fog_checkbox_label(settings.0.fog_of_war);
    for children in &boxes {
        if let Some(mut t) = children.first().and_then(|c| texts.get_mut(*c).ok())
            && t.0 != want
        {
            want.clone_into(&mut t.0);
        }
    }
}

fn escape_goes_back(
    keys: Res<ButtonInput<KeyCode>>,
    state: Res<State<Screen>>,
    mut next: ResMut<NextState<Screen>>,
) {
    if keys.just_pressed(KeyCode::Escape)
        && let Some(back) = state.get().back()
    {
        next.set(back);
    }
}

/// Enter in the agents-folder field saves (the field takes no newlines, so
/// the key reaches here).
fn enter_saves(
    keys: Res<ButtonInput<KeyCode>>,
    focus: Res<InputFocus>,
    fields: Query<(), With<AgentsDirField>>,
    mut commands: Commands,
) {
    if keys.just_pressed(KeyCode::Enter) && focus.get().is_some_and(|e| fields.contains(e)) {
        commands.queue(|world: &mut World| apply_action(world, Action::SaveSettings));
    }
}

/// Adds a button with its [`Action`] and the activate observer.
pub(crate) fn button(
    parent: &mut ChildSpawnerCommands,
    text: &str,
    detail: Option<&str>,
    enabled: bool,
    tab: i32,
    action: Action,
) -> Entity {
    let id = ui::button(parent, text, detail, enabled, tab, action);
    parent.commands().entity(id).observe(on_activate);
    id
}

pub(crate) fn back_button(parent: &mut ChildSpawnerCommands, screen: Screen, tab: i32) {
    if let Some(back) = screen.back() {
        button(parent, "Back", None, true, tab, Action::Go(back));
    }
}

fn spawn_main(mut commands: Commands, settings: Res<AppSettings>, notice: Res<MainNotice>) {
    let explorer = launch::explorer_enabled(&settings.0);
    let panel = ui::spawn_screen(&mut commands, DespawnOnExit(Screen::Main), "npc-framework");
    commands.entity(panel).with_children(|p| {
        p.spawn(ui::label("Pick something to play.", 16.0, ui::DIM));
        let hint = explorer.as_ref().err().map(String::as_str);
        let first = button(
            p,
            "Explore with an AI agent",
            Some(hint.unwrap_or("an AI agent explores a world; you watch")),
            explorer.is_ok(),
            0,
            Action::Go(Screen::Explorer),
        );
        let testbed = button(
            p,
            "Actor Testbed",
            Some("every capability, one scenario each"),
            true,
            1,
            Action::Go(Screen::Testbed),
        );
        p.commands()
            .entity(if explorer.is_ok() { first } else { testbed })
            .insert(AutoFocus);
        button(
            p,
            "Village",
            Some("villagers with needs and a food economy"),
            true,
            2,
            Action::Launch(Entry::Village),
        );
        button(
            p,
            "RTS units demo",
            Some("a foreign host: its own units, map and physics"),
            true,
            3,
            Action::Launch(Entry::RtsDemo),
        );
        button(p, "Settings", None, true, 4, Action::Go(Screen::Settings));
        button(
            p,
            "Quit",
            Some("stops every world and window it started (they save first)"),
            true,
            5,
            Action::Quit,
        );
        if let Some(e) = &notice.0 {
            p.spawn(ui::label(e.clone(), 16.0, ERROR));
        }
    });
}

fn spawn_testbed(mut commands: Commands) {
    let panel = ui::spawn_screen(
        &mut commands,
        DespawnOnExit(Screen::Testbed),
        "Actor Testbed",
    );
    commands.entity(panel).with_children(|p| {
        p.spawn(ui::label("Pick a scenario.", 16.0, ui::DIM));
        p.spawn((
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: px(6),
                overflow: Overflow::scroll_y(),
                flex_grow: 1.0,
                flex_shrink: 1.0,
                min_height: px(0),
                ..default()
            },
            ScrollArea,
        ))
        .with_children(|list| {
            for (i, (name, desc)) in launch::testbed_scenarios().enumerate() {
                let tab = i32::try_from(i).unwrap_or(i32::MAX);
                let id = ui::list_button(
                    list,
                    (name, ui::NAME_COLUMN_PX),
                    desc,
                    true,
                    tab,
                    Action::Launch(Entry::Testbed(name)),
                );
                list.commands().entity(id).observe(on_activate);
                if i == 0 {
                    list.commands().entity(id).insert(AutoFocus);
                }
            }
        });
        ui::row(p, |r| back_button(r, Screen::Testbed, 100));
    });
}

fn spawn_settings(mut commands: Commands, settings: Res<AppSettings>, paths: Res<Paths>) {
    let panel = ui::spawn_screen(&mut commands, DespawnOnExit(Screen::Settings), "Settings");
    let current = settings.0.agents_dir.display().to_string();
    let file = paths.settings_file.as_ref().map_or_else(
        || "(no settings folder)".to_owned(),
        |f| f.display().to_string(),
    );
    commands.entity(panel).with_children(|p| {
        p.spawn(ui::label("Agents folder", 18.0, TEXT));
        p.spawn(ui::label(
            "Every AI agent gets a folder in here (its prompts, diary, proposals and world save).",
            14.0,
            ui::DIM,
        ));
        p.spawn((AgentsDirField, ui::text_field(current, 0), AutoFocus));
        p.spawn(ui::label("Explorer window", 18.0, TEXT));
        let fog = button(
            p,
            fog_checkbox_label(settings.0.fog_of_war),
            Some(
                "Show the world as the agent's actor knows it: black where it has never been, greyed as last seen where it is not looking, no units it cannot sense. F8 toggles it in the game.",
            ),
            true,
            1,
            Action::ToggleFog,
        );
        p.commands().entity(fog).insert(FogCheckbox);
        p.spawn((SettingsStatusText, ui::label("", 14.0, NOTE)));
        p.spawn(ui::label(format!("Saved to {file}"), 12.0, ui::DIM));
        ui::row(p, |r| {
            button(r, "Save", None, true, 2, Action::SaveSettings);
            back_button(r, Screen::Settings, 3);
        });
    });
}

fn show_settings_status(
    status: Res<SettingsStatus>,
    mut text: Query<(&mut Text, &mut TextColor), With<SettingsStatusText>>,
) {
    if !status.is_changed() {
        return;
    }
    let (line, color) = match &status.0 {
        None => (String::new(), NOTE),
        Some(Ok(m)) => (m.clone(), OK),
        Some(Err(e)) => (e.clone(), ERROR),
    };
    for (mut t, mut c) in &mut text {
        t.0.clone_from(&line);
        c.0 = color;
    }
}

fn spawn_explorer(mut commands: Commands, settings: Res<AppSettings>) {
    let panel = ui::spawn_screen(
        &mut commands,
        DespawnOnExit(Screen::Explorer),
        "Explore with an AI agent",
    );
    let dir = settings.0.agents_dir.display().to_string();
    commands.entity(panel).with_children(|p| {
        p.spawn(ui::label(format!("Agents folder: {dir}"), 14.0, ui::DIM));
        let new = button(
            p,
            "New agent",
            Some("name an agent and its world"),
            true,
            0,
            Action::Go(Screen::ExplorerNew),
        );
        p.commands().entity(new).insert(AutoFocus);
        button(
            p,
            "Resume an agent",
            Some("continue a world where it was left"),
            true,
            1,
            Action::Go(Screen::ExplorerResume),
        );
        ui::row(p, |r| back_button(r, Screen::Explorer, 2));
    });
}

fn spawn_running(mut commands: Commands, running: Option<Res<Running>>) {
    let (heading, line) = running.as_ref().map_or_else(
        || ("Running", "starting...".to_owned()),
        |r| (r.heading(), r.status()),
    );
    let building = running.is_some_and(|r| r.is_build());
    let panel = ui::spawn_screen(&mut commands, DespawnOnExit(Screen::Running), heading);
    commands.entity(panel).with_children(|p| {
        p.spawn(ui::label(line, 18.0, TEXT));
        if building {
            ui::row(p, |r| {
                let cancel = button(r, "Cancel", None, true, 0, Action::CancelBuild);
                r.commands().entity(cancel).insert(AutoFocus);
            });
        }
    });
}

fn spawn_missing(mut commands: Commands, missing: Option<Res<Missing>>, tools: Res<BuildTools>) {
    let Some(missing) = missing else {
        commands.queue(|w: &mut World| run::set_screen(w, Screen::Main));
        return;
    };
    let panel = ui::spawn_screen(
        &mut commands,
        DespawnOnExit(Screen::Missing),
        &format!("{} is not built", missing.stem),
    );
    let can_build = tools.cargo.is_some() && tools.root.is_some();
    commands.entity(panel).with_children(|p| {
        p.spawn(ui::label("Build it with:", 16.0, TEXT));
        p.spawn((
            Node {
                padding: UiRect::all(px(8)),
                border_radius: BorderRadius::all(px(4)),
                ..default()
            },
            BackgroundColor(ui::FIELD),
        ))
        .with_child(ui::label(missing.command.clone(), 16.0, TEXT));
        let hint = match (&tools.cargo, &tools.root) {
            (Some(_), Some(root)) => format!("Build runs it in {}.", root.display()),
            (None, _) => {
                "cargo is not on PATH: run the command in the workspace, then try again.".to_owned()
            }
            (_, None) => {
                "The workspace was not found: run the command there, then try again.".to_owned()
            }
        };
        p.spawn(ui::label(hint, 14.0, ui::DIM));
        if let Some(e) = &missing.error {
            p.spawn(ui::label(e.clone(), 14.0, ERROR));
        }
        ui::row(p, |r| {
            if can_build {
                let b = button(r, "Build", None, true, 0, Action::Build);
                r.commands().entity(b).insert(AutoFocus);
            }
            back_button(r, Screen::Missing, 1);
        });
    });
}

/// The settings file the menu reads and writes: `--settings FILE`, else the
/// per-user one.
pub fn settings_file(flag: Option<PathBuf>) -> Option<PathBuf> {
    flag.or_else(|| testbed_content::explorer::settings::Settings::file(&|k| std::env::var(k).ok()))
}

#[cfg(test)]
pub(crate) mod tests {
    use std::process::Command;

    use bevy::state::app::StatesPlugin;
    use bevy::ui::InteractionDisabled;
    use testbed_content::explorer::settings::Settings;

    use super::*;
    use crate::run::Job;

    pub(crate) fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("game-menu-ui-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// The menu without a window: the screens plugin over `exe_dir`.
    pub(crate) fn app(exe_dir: &Path, agents_dir: &Path) -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatesPlugin))
            .init_resource::<InputFocus>()
            .init_resource::<ButtonInput<KeyCode>>()
            .insert_resource(AppSettings(Settings {
                agents_dir: agents_dir.to_path_buf(),
                fog_of_war: true,
                actors_dir: None,
            }))
            .insert_resource(BuildTools {
                cargo: None,
                root: None,
            })
            .insert_resource(Paths {
                exe_dir: exe_dir.to_path_buf(),
                settings_file: None,
            })
            .add_plugins(ScreensPlugin {
                start: Screen::Main,
            });
        app.update();
        app
    }

    pub(crate) fn screen(app: &App) -> Screen {
        *app.world().resource::<State<Screen>>().get()
    }

    /// The button whose action matches `want`.
    pub(crate) fn button_for(app: &mut App, want: impl Fn(&Action) -> bool) -> Entity {
        let mut q = app.world_mut().query::<(Entity, &Action)>();
        let found: Vec<Entity> = q
            .iter(app.world())
            .filter(|(_, a)| want(a))
            .map(|(e, _)| e)
            .collect();
        assert_eq!(found.len(), 1, "exactly one matching button");
        found[0]
    }

    pub(crate) fn press(app: &mut App, entity: Entity) {
        app.world_mut().trigger(Activate { entity });
        app.world_mut().flush();
        app.update();
    }

    #[test]
    fn screen_names_and_back() {
        for name in [
            "main",
            "testbed",
            "settings",
            "explorer",
            "explorer_new",
            "explorer_resume",
        ] {
            assert!(Screen::from_name(name).is_some(), "{name}");
            assert!(Screen::NAMES.contains(name));
        }
        assert_eq!(Screen::from_name("running"), None);
        assert_eq!(Screen::Testbed.back(), Some(Screen::Main));
        assert_eq!(Screen::ExplorerNew.back(), Some(Screen::Explorer));
        assert_eq!(Screen::Main.back(), None);
        assert_eq!(Screen::Running.back(), None);
    }

    #[test]
    fn buttons_switch_screens_and_back_returns() {
        let d = tmp("nav");
        let mut app = app(&d, &d);
        let settings = button_for(&mut app, |a| matches!(a, Action::Go(Screen::Settings)));
        press(&mut app, settings);
        assert_eq!(screen(&app), Screen::Settings);
        // Main's buttons are gone, Settings has Save and Back.
        let back = button_for(&mut app, |a| matches!(a, Action::Go(Screen::Main)));
        button_for(&mut app, |a| matches!(a, Action::SaveSettings));
        press(&mut app, back);
        assert_eq!(screen(&app), Screen::Main);
        let testbed = button_for(&mut app, |a| matches!(a, Action::Go(Screen::Testbed)));
        press(&mut app, testbed);
        assert_eq!(screen(&app), Screen::Testbed);
        let mut q = app.world_mut().query::<&Action>();
        let launches = q
            .iter(app.world())
            .filter(|a| matches!(a, Action::Launch(Entry::Testbed(_))))
            .count();
        assert_eq!(launches, launch::testbed_scenarios().count());
        let _ = std::fs::remove_dir_all(&d);
    }

    /// The Settings screen's fog checkbox flips the setting, saves it and
    /// shows the box; Save keeps it.
    #[test]
    fn fog_checkbox_toggles_and_saves() {
        let d = tmp("fog");
        let mut app = app(&d, &d);
        let file = d.join("settings.json");
        app.world_mut().resource_mut::<Paths>().settings_file = Some(file.clone());
        let settings = button_for(&mut app, |a| matches!(a, Action::Go(Screen::Settings)));
        press(&mut app, settings);
        let label = |app: &mut App| {
            let mut q = app
                .world_mut()
                .query_filtered::<&Children, With<FogCheckbox>>();
            let first = q.single(app.world()).unwrap()[0];
            app.world().get::<Text>(first).unwrap().0.clone()
        };
        assert_eq!(label(&mut app), "[x] Fog of war");
        let fog = button_for(&mut app, |a| matches!(a, Action::ToggleFog));
        press(&mut app, fog);
        app.update();
        assert!(!app.world().resource::<AppSettings>().0.fog_of_war);
        assert_eq!(label(&mut app), "[ ] Fog of war");
        assert!(!Settings::load(&file, &d).fog_of_war, "saved at once");
        let save = button_for(&mut app, |a| matches!(a, Action::SaveSettings));
        press(&mut app, save);
        assert!(!Settings::load(&file, &d).fog_of_war, "Save keeps it");
        press(&mut app, fog);
        assert!(Settings::load(&file, &d).fog_of_war);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn explorer_entry_disabled_without_valid_folder() {
        let d = tmp("explorer");
        let mut app = app(&d, Path::new("relative/agents"));
        let explore = button_for(&mut app, |a| matches!(a, Action::Go(Screen::Explorer)));
        assert!(
            app.world()
                .entity(explore)
                .contains::<InteractionDisabled>()
        );
        let mut app = app_with_valid(&d);
        let explore = button_for(&mut app, |a| matches!(a, Action::Go(Screen::Explorer)));
        assert!(
            !app.world()
                .entity(explore)
                .contains::<InteractionDisabled>()
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    fn app_with_valid(d: &Path) -> App {
        app(d, &d.join("agents"))
    }

    #[test]
    fn missing_game_shows_its_build_command() {
        let d = tmp("missing");
        let mut app = app_with_valid(&d);
        let village = button_for(&mut app, |a| matches!(a, Action::Launch(Entry::Village)));
        press(&mut app, village);
        app.update();
        assert_eq!(screen(&app), Screen::Missing);
        let missing = app.world().resource::<Missing>();
        assert_eq!(missing.stem, "village");
        assert_eq!(missing.command, "cargo build -p village");
        // No cargo or workspace in this test: no Build button.
        let mut q = app.world_mut().query::<&Action>();
        assert!(!q.iter(app.world()).any(|a| matches!(a, Action::Build)));
        let _ = std::fs::remove_dir_all(&d);
    }

    /// The environment variable that makes this test binary a fake child
    /// ([`child_process`]).
    const CHILD_MODE: &str = "GAME_MENU_TEST_CHILD";

    /// A command running this test binary as a child that prints `stdout`
    /// and `stderr`, then exits with `code`, or sleeps for minutes (`None`:
    /// a hung game, build or check).
    pub(crate) fn test_child(code: Option<i32>, stdout: &str, stderr: &str) -> Command {
        test_child_until(code, stdout, stderr, None)
    }

    /// [`test_child`] that, after printing, waits for the file `until` to
    /// exist (at most 120 s) before it exits: a detached world that runs
    /// until its fake `session/stop` writes the file.
    pub(crate) fn test_child_until(
        code: Option<i32>,
        stdout: &str,
        stderr: &str,
        until: Option<&Path>,
    ) -> Command {
        let mut cmd = Command::new(std::env::current_exe().unwrap());
        cmd.args([
            "--exact",
            "screens::tests::child_process",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(
            CHILD_MODE,
            code.map_or_else(|| "hang".to_owned(), |c| c.to_string()),
        )
        .env("GAME_MENU_TEST_STDOUT", stdout)
        .env("GAME_MENU_TEST_STDERR", stderr);
        if let Some(until) = until {
            cmd.env("GAME_MENU_TEST_UNTIL", until);
        }
        cmd
    }

    /// Not a test: the fake child of [`test_child`] (does nothing in a normal
    /// test run).
    #[test]
    fn child_process() {
        use std::io::Write as _;
        let Ok(mode) = std::env::var(CHILD_MODE) else {
            return;
        };
        let out = std::env::var("GAME_MENU_TEST_STDOUT").unwrap_or_default();
        let err = std::env::var("GAME_MENU_TEST_STDERR").unwrap_or_default();
        // libtest's "test ... " line has no line break yet.
        let mut stdout = std::io::stdout();
        let _ = write!(stdout, "\n{out}");
        let _ = stdout.flush();
        let mut stderr = std::io::stderr();
        let _ = stderr.write_all(err.as_bytes());
        let _ = stderr.flush();
        if let Ok(until) = std::env::var("GAME_MENU_TEST_UNTIL") {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
            while !Path::new(&until).exists() && std::time::Instant::now() < deadline {
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
        }
        if mode == "hang" {
            std::thread::sleep(std::time::Duration::from_secs(300));
        }
        std::process::exit(mode.parse().unwrap_or(99));
    }

    /// Waits on the Running screen until the child is reaped.
    fn run_child(app: &mut App, args: &[&str]) {
        run_job(app, args, Job::Game(Entry::Village));
    }

    fn run_job(app: &mut App, args: &[&str], job: Job) {
        let mut cmd = Command::new(std::env::current_exe().unwrap());
        cmd.args(args).stderr(std::process::Stdio::null());
        run_command(app, cmd, job);
    }

    /// Runs `cmd` (stdout discarded) as the Running screen's child and waits
    /// until it is reaped.
    fn run_command(app: &mut App, mut cmd: Command, job: Job) {
        let child = cmd.stdout(std::process::Stdio::null()).spawn().unwrap();
        app.insert_resource(Running::new(child, job));
        run::set_screen(app.world_mut(), Screen::Running);
        app.update();
        assert_eq!(screen(app), Screen::Running);
        for _ in 0..2000 {
            app.update();
            if !app.world().contains_resource::<Running>() {
                app.update();
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        panic!("the child never exited");
    }

    #[test]
    fn a_failed_game_returns_to_main_with_its_exit_code() {
        let d = tmp("exit");
        let mut app = app_with_valid(&d);
        // libtest rejects an unknown flag with a non-zero exit code.
        run_child(&mut app, &["--no-such-flag"]);
        assert_eq!(screen(&app), Screen::Main);
        let notice = app.world().resource::<MainNotice>().0.clone().unwrap();
        assert!(notice.starts_with("Village exited with code "), "{notice}");
        // A clean exit clears it.
        run_child(&mut app, &["--list"]);
        assert_eq!(screen(&app), Screen::Main);
        assert_eq!(app.world().resource::<MainNotice>().0, None);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_failed_game_shows_its_last_stderr_line() {
        let d = tmp("exit-reason");
        let mut app = app_with_valid(&d);
        // What `start_game` pipes: the game's stderr.
        let mut cmd = test_child(
            Some(3),
            "",
            "INFO loading\nERROR the world file is gone\n\n\
             note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace\n",
        );
        cmd.stderr(std::process::Stdio::piped());
        run_command(&mut app, cmd, Job::Game(Entry::Village));
        assert_eq!(screen(&app), Screen::Main);
        assert_eq!(
            app.world().resource::<MainNotice>().0.as_deref(),
            Some("Village exited with code 3: ERROR the world file is gone")
        );
        let mut q = app.world_mut().query::<&Text>();
        assert!(
            q.iter(app.world())
                .any(|t| t.0 == "Village exited with code 3: ERROR the world file is gone"),
            "shown on Main"
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    /// Starts a hung child as a build on the Building screen.
    fn start_hung_build(app: &mut App) {
        app.insert_resource(Missing::new("village", Entry::Village, Path::new("/x")));
        let child = test_child(None, "", "")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let job = Job::Build {
            entry: Entry::Village,
            stem: "village",
        };
        app.insert_resource(Running::new(child, job));
        run::set_screen(app.world_mut(), Screen::Running);
        app.update();
        app.update();
        assert_eq!(screen(app), Screen::Running);
    }

    #[test]
    fn cancel_stops_the_build_and_returns_to_missing() {
        let d = tmp("cancel");
        let mut app = app_with_valid(&d);
        start_hung_build(&mut app);
        let started = std::time::Instant::now();
        let cancel = button_for(&mut app, |a| matches!(a, Action::CancelBuild));
        press(&mut app, cancel);
        app.update();
        // Removing `Running` killed and reaped the child (else it sleeps on).
        assert!(started.elapsed() < std::time::Duration::from_secs(60));
        assert!(!app.world().contains_resource::<Running>());
        assert_eq!(screen(&app), Screen::Missing);
        assert_eq!(
            app.world().resource::<Missing>().error.as_deref(),
            Some("the build was cancelled")
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_game_has_no_cancel_and_cancel_leaves_it_alone() {
        let d = tmp("no-cancel");
        let mut app = app_with_valid(&d);
        let child = Command::new(std::env::current_exe().unwrap())
            .arg("--list")
            .stdout(std::process::Stdio::null())
            .spawn()
            .unwrap();
        app.insert_resource(Running::new(child, Job::Game(Entry::Village)));
        run::set_screen(app.world_mut(), Screen::Running);
        app.update();
        let mut q = app.world_mut().query::<&Action>();
        assert!(
            !q.iter(app.world())
                .any(|a| matches!(a, Action::CancelBuild))
        );
        run::cancel_build(app.world_mut());
        assert!(app.world().contains_resource::<Running>());
        let _ = app.world_mut().resource_mut::<Running>().child.wait();
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn the_update_mode_follows_what_the_menu_does() {
        use bevy::window::RequestRedraw;
        use bevy::winit::{UpdateMode, WinitSettings};

        use crate::power::{BLINK, IDLE, POLL, PowerPlugin, SHOT_POLL};

        let d = tmp("power");
        let mut app = app_with_valid(&d);
        app.add_plugins(PowerPlugin);
        let focused = |app: &App| app.world().resource::<WinitSettings>().focused_mode;
        app.update();
        // Right after start: polled while the first frames lay the UI out.
        assert_eq!(focused(&app), UpdateMode::reactive_low_power(SHOT_POLL));
        std::thread::sleep(std::time::Duration::from_millis(1100));
        app.update();
        // Then reactive: a frame on input only.
        assert_eq!(focused(&app), UpdateMode::reactive_low_power(IDLE));
        // A key press asks for one more frame (its action lands at the end
        // of the frame).
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Tab);
        app.update();
        assert!(!app.world().resource::<Messages<RequestRedraw>>().is_empty());
        // A focused text field: frames for the caret's blink.
        run::set_screen(app.world_mut(), Screen::Settings);
        app.update();
        let mut q = app
            .world_mut()
            .query_filtered::<Entity, With<AgentsDirField>>();
        let field = q.single(app.world()).unwrap();
        app.world_mut()
            .resource_mut::<InputFocus>()
            .set(field, bevy::input_focus::FocusCause::Navigated);
        app.update();
        assert_eq!(focused(&app), UpdateMode::reactive_low_power(BLINK));
        // Building: the Building screen polls cargo.
        start_hung_build(&mut app);
        app.update();
        assert_eq!(focused(&app), UpdateMode::reactive_low_power(POLL));
        run::cancel_build(app.world_mut());
        // A game: the window is hidden, the menu only polls.
        let child = test_child(None, "", "")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap();
        app.insert_resource(Running::new(child, Job::Game(Entry::Village)));
        app.update();
        assert_eq!(focused(&app), UpdateMode::reactive_low_power(POLL));
        let mut game = app.world_mut().remove_resource::<Running>().unwrap();
        run::kill(&mut game.child);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn closing_the_menu_stops_the_build() {
        let d = tmp("close");
        let mut app = app_with_valid(&d);
        start_hung_build(&mut app);
        let started = std::time::Instant::now();
        // What closing the last window writes.
        app.world_mut().write_message(AppExit::Success);
        app.update();
        assert!(!app.world().contains_resource::<Running>());
        assert!(started.elapsed() < std::time::Duration::from_secs(60));
        let _ = std::fs::remove_dir_all(&d);
    }
    fn status(app: &App) -> Option<Result<String, String>> {
        app.world().resource::<SettingsStatus>().0.clone()
    }

    fn type_agents_dir(app: &mut App, text: &str) {
        let mut q = app
            .world_mut()
            .query_filtered::<&mut EditableText, With<AgentsDirField>>();
        let mut field = q.single_mut(app.world_mut()).unwrap();
        *field = EditableText::new(text);
    }

    #[test]
    fn save_validates_inline_then_creates_and_persists() {
        let d = tmp("save");
        let mut app = app_with_valid(&d);
        let file = d.join("cfg").join("settings.json");
        app.world_mut().resource_mut::<Paths>().settings_file = Some(file.clone());
        run::set_screen(app.world_mut(), Screen::Settings);
        app.update();
        let save = button_for(&mut app, |a| matches!(a, Action::SaveSettings));
        type_agents_dir(&mut app, "not/absolute");
        press(&mut app, save);
        let e = status(&app).unwrap().unwrap_err();
        assert!(e.contains("absolute"), "{e}");
        assert!(!file.exists());
        let mut q = app
            .world_mut()
            .query_filtered::<&Text, With<SettingsStatusText>>();
        assert_eq!(q.single(app.world()).unwrap().0, e, "shown inline");
        let agents = d.join("new").join("agents");
        type_agents_dir(&mut app, &agents.display().to_string());
        press(&mut app, save);
        assert!(status(&app).unwrap().is_ok());
        assert!(agents.is_dir());
        assert_eq!(app.world().resource::<AppSettings>().0.agents_dir, agents);
        assert_eq!(Settings::load(&file, &d).agents_dir, agents);
        let _ = std::fs::remove_dir_all(&d);
    }
    #[test]
    fn a_build_that_leaves_the_game_missing_says_so() {
        let d = tmp("built");
        let mut app = app_with_valid(&d);
        app.insert_resource(Missing::new("village", Entry::Village, &d));
        let job = Job::Build {
            entry: Entry::Village,
            stem: "village",
        };
        // A child that exits 0 stands in for cargo.
        run_job(&mut app, &["--list"], job);
        assert_eq!(screen(&app), Screen::Missing);
        let e = app.world().resource::<Missing>().error.clone().unwrap();
        assert!(e.starts_with("built, but village is still not in"), "{e}");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn enter_in_the_field_saves() {
        let d = tmp("enter");
        let mut app = app_with_valid(&d);
        app.world_mut().resource_mut::<Paths>().settings_file = Some(d.join("s.json"));
        run::set_screen(app.world_mut(), Screen::Settings);
        app.update();
        let mut q = app
            .world_mut()
            .query_filtered::<Entity, With<AgentsDirField>>();
        let field = q.single(app.world()).unwrap();
        app.world_mut()
            .resource_mut::<InputFocus>()
            .set(field, bevy::input_focus::FocusCause::Navigated);
        type_agents_dir(&mut app, &d.join("via-enter").display().to_string());
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Enter);
        app.update();
        app.update();
        assert!(
            status(&app).is_some_and(|s| s.is_ok()),
            "{:?}",
            status(&app)
        );
        assert!(d.join("via-enter").is_dir());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_second_launch_while_running_is_ignored() {
        let d = tmp("twice");
        let mut app = app_with_valid(&d);
        let child = Command::new(std::env::current_exe().unwrap())
            .arg("--list")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let pid = child.id();
        app.insert_resource(Running::new(child, Job::Game(Entry::RtsDemo)));
        // Two activations in one frame: neither replaces (orphans) the child.
        apply_action(app.world_mut(), Action::Launch(Entry::Village));
        apply_action(
            app.world_mut(),
            Action::Launch(Entry::ExplorerResume {
                agent: "Wren".into(),
                choice: None,
                mode: RunMode::Window,
            }),
        );
        run::build(
            app.world_mut(),
            Entry::Village,
            "village",
            Path::new("cargo"),
            &d,
        );
        assert_eq!(app.world().resource::<Running>().child.id(), pid);
        assert!(!app.world().contains_resource::<Missing>());
        assert!(
            !app.world()
                .contains_resource::<crate::explorer::SaveCheck>()
        );
        let _ = app.world_mut().resource_mut::<Running>().child.wait();
        let _ = std::fs::remove_dir_all(&d);
    }
}
