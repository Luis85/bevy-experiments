//! Application flow and the command boundary between UI and simulation.

use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

use battle_core::{
    ArmyConfig, Formation, MapKind, SIM_DT, SIM_HZ, ScenarioConfig, Team, UnitCategory, UnitKind,
};
use bevy::{app::AppExit, input_focus::InputFocus, prelude::*, window::PresentMode};

use crate::runtime::{BattleSnapshot, SimulationWorker, WorkerReply};

#[derive(States, Default, Debug, Clone, Eq, PartialEq, Hash)]
pub enum Screen {
    #[default]
    Menu,
    Configure,
    Battle,
}

#[derive(SystemSet, Debug, Clone, Eq, PartialEq, Hash)]
pub enum AppSet {
    Input,
    Actions,
    Simulation,
    Presentation,
}

#[derive(Resource)]
pub struct ScenarioDraft(pub ScenarioConfig);

/// The worker owns authoritative state; the application reads immutable snapshots.
#[derive(Resource)]
pub struct Session {
    pub battle: Option<Arc<BattleSnapshot>>,
    pub paused: bool,
    pub speed: f32,
    pub accumulator: f64,
    pub generation: u64,
    pub single_step: bool,
    pub diagnostics: bool,
    pub help: bool,
    pub status: String,
    pub last_frame_ms: f32,
    pub smoothed_fps: f32,
    pub dropped_time: f64,
    pub preparing: bool,
    pub busy: bool,
    pub faulted: bool,
    pub snapshot_ms: f32,
    worker: SimulationWorker,
    requested_generation: u64,
    presentation_age: f32,
    publication_elapsed: f32,
    presentation_interval: f32,
    pending_status: String,
}

impl Session {
    pub fn new(config: ScenarioConfig) -> Result<Self, String> {
        config.validate()?;
        let worker = SimulationWorker::new()?;
        worker.reset(1, config.clone())?;
        let status = format!("Erzeuge {} Einheiten …", config.total_units());
        Ok(Self {
            battle: None,
            paused: true,
            speed: 1.0,
            accumulator: 0.0,
            generation: 0,
            requested_generation: 1,
            single_step: false,
            diagnostics: false,
            help: false,
            pending_status: status.clone(),
            status,
            last_frame_ms: 0.0,
            smoothed_fps: 60.0,
            dropped_time: 0.0,
            preparing: true,
            busy: true,
            faulted: false,
            snapshot_ms: 0.0,
            worker,
            presentation_age: 0.0,
            publication_elapsed: 0.0,
            presentation_interval: SIM_DT,
        })
    }

    fn reset(&mut self, config: ScenarioConfig, paused: bool) -> Result<(), String> {
        config.validate()?;
        let generation = self.requested_generation.wrapping_add(1);
        let count = config.total_units();
        self.worker.reset(generation, config)?;
        self.requested_generation = generation;
        self.paused = paused;
        self.speed = 1.0;
        self.accumulator = 0.0;
        self.single_step = false;
        self.dropped_time = 0.0;
        self.preparing = true;
        self.faulted = false;
        self.publication_elapsed = 0.0;
        self.presentation_age = self.presentation_interval;
        self.busy = true;
        self.pending_status = format!("Erzeuge {count} Einheiten …");
        self.status = self.pending_status.clone();
        Ok(())
    }

    pub fn is_ready(&self) -> bool {
        !self.faulted
            && !self.preparing
            && self.generation == self.requested_generation
            && self.battle.is_some()
    }

    pub fn set_paused(&mut self, paused: bool) {
        self.paused = paused;
        self.accumulator = 0.0;
        self.publication_elapsed = 0.0;
        self.presentation_age = self.presentation_interval;
        if paused {
            self.single_step = false;
            self.worker.stop_after_current_tick();
        }
    }

    pub fn interpolation(&self) -> f32 {
        if self.paused {
            1.0
        } else {
            (self.presentation_age / self.presentation_interval.max(0.001)).clamp(0.0, 1.0)
        }
    }

    pub fn submit_ticks(&mut self, ticks: u32) -> Result<(), String> {
        if !self.is_ready() || self.busy {
            return Err("Die Simulation berechnet noch den aktuellen Auftrag.".into());
        }
        self.worker.advance(self.generation, ticks)?;
        self.busy = true;
        Ok(())
    }

    fn receive_worker(&mut self) {
        let Some(reply) = self.worker.take_reply() else {
            return;
        };
        match reply {
            WorkerReply::Snapshot {
                generation,
                snapshot,
            } if generation == self.requested_generation => {
                let fresh_scenario = self.generation != generation;
                let target_interval = snapshot.span_ticks.max(1) as f32 * SIM_DT / self.speed;
                self.presentation_interval = if fresh_scenario {
                    target_interval
                } else {
                    self.publication_elapsed.max(target_interval).min(1.0)
                };
                self.presentation_age = 0.0;
                self.publication_elapsed = 0.0;
                self.faulted = false;
                self.generation = generation;
                self.preparing = false;
                self.busy = false;
                self.snapshot_ms = snapshot.snapshot_ms;
                if self.status == self.pending_status {
                    self.status.clear();
                }
                if snapshot.outcome.is_some() {
                    self.paused = true;
                    self.accumulator = 0.0;
                    self.single_step = false;
                }
                if self.paused {
                    self.presentation_age = self.presentation_interval;
                }
                self.battle = Some(snapshot);
            }
            WorkerReply::Failed {
                generation,
                message,
            } if generation == self.requested_generation => {
                self.preparing = false;
                self.busy = false;
                self.paused = true;
                self.single_step = false;
                self.faulted = true;
                self.status = message;
                error!("{}", self.status);
            }
            _ => {} // A newer reset owns the application; stale replies are ignored.
        }
    }
}

#[derive(Message, Clone, Debug, PartialEq)]
pub enum AppAction {
    Configure,
    QuickBattle,
    StartBattle,
    MainMenu,
    TogglePause,
    Step,
    Restart,
    SetSpeed(f32),
    ToggleDiagnostics,
    ToggleHelp,
    Quit,
    Preset(usize),
    SetMap(MapKind),
    SetKind(Team, UnitKind),
    SetCategory(Team, UnitCategory),
    SetFormation(Team, Formation),
    ChangeCount(Team, i32),
    ChangeSeed(i64),
    ChangeDuration(i32),
    SaveScenario,
    LoadScenario,
}

#[derive(Resource, Clone)]
pub struct LaunchOptions {
    pub data_dir: PathBuf,
    pub capture_dir: PathBuf,
    pub smoke_test: bool,
    pub render_check: bool,
    pub width: u32,
    pub height: u32,
    pub quick_battle: bool,
    pub scenario: Option<PathBuf>,
}

impl LaunchOptions {
    fn parse() -> Result<Option<Self>, String> {
        let mut options = Self {
            data_dir: PathBuf::from("user-data"),
            capture_dir: PathBuf::from("captures"),
            smoke_test: false,
            render_check: false,
            width: 1440,
            height: 900,
            quick_battle: false,
            scenario: None,
        };
        let mut args = std::env::args().skip(1);
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--help" | "-h" => {
                    println!(
                        "Legion Battle Lab\n\ncargo run --release -p legion -- [OPTIONEN]\n\n  --quick                 Direkt mit der Schlacht starten\n  --scenario DATEI        JSON-Szenario laden\n  --data-dir ORDNER       Szenarien hier speichern (Standard: user-data)\n  --width PIXEL           Fensterbreite (mindestens 1024)\n  --height PIXEL          Fensterhöhe (mindestens 768)\n  --smoke-test            Automatischer Test der Bildschirmzustände\n  --render-check          Szenario, GPU-Population und Screenshot prüfen\n  --capture-dir ORDNER    Ausgabeordner für native Prüfungen\n\nOhne Grafik: cargo run --release -p battle_core --bin battle-bench -- --help"
                    );
                    return Ok(None);
                }
                "--quick" => options.quick_battle = true,
                "--smoke-test" => options.smoke_test = true,
                "--render-check" => options.render_check = true,
                "--scenario" => {
                    options.scenario = Some(PathBuf::from(
                        args.next().ok_or("--scenario benötigt einen Dateinamen")?,
                    ))
                }
                "--data-dir" => {
                    options.data_dir =
                        PathBuf::from(args.next().ok_or("--data-dir benötigt einen Ordner")?)
                }
                "--capture-dir" => {
                    options.capture_dir =
                        PathBuf::from(args.next().ok_or("--capture-dir benötigt einen Ordner")?)
                }
                "--width" => {
                    options.width = args
                        .next()
                        .ok_or("--width benötigt eine Zahl")?
                        .parse::<u32>()
                        .map_err(|_| "Ungültige Fensterbreite")?
                        .clamp(1024, 7680)
                }
                "--height" => {
                    options.height = args
                        .next()
                        .ok_or("--height benötigt eine Zahl")?
                        .parse::<u32>()
                        .map_err(|_| "Ungültige Fensterhöhe")?
                        .clamp(768, 4320)
                }
                _ => {
                    return Err(format!(
                        "Unbekanntes Argument: {arg}. --help zeigt die Optionen."
                    ));
                }
            }
        }
        if options.smoke_test && options.render_check {
            return Err("--smoke-test und --render-check bitte getrennt ausführen.".into());
        }
        Ok(Some(options))
    }
}

pub fn run() -> Result<(), String> {
    let Some(mut options) = LaunchOptions::parse()? else {
        return Ok(());
    };
    let mut config = if let Some(path) = &options.scenario {
        read_scenario(path)?
    } else {
        ScenarioConfig::default()
    };
    if options.smoke_test {
        options.data_dir = options.capture_dir.join("smoke-data");
        config.blue.count = 128;
        config.red.count = 128;
        fs::create_dir_all(&options.capture_dir).map_err(|e| format!("Screenshot-Ordner: {e}"))?;
    }
    if options.render_check {
        fs::create_dir_all(&options.capture_dir).map_err(|e| format!("Screenshot-Ordner: {e}"))?;
    }
    config.validate()?;
    let mut session = Session::new(config.clone())?;
    session.diagnostics = options.smoke_test;
    let start_battle = options.quick_battle || options.render_check;
    if start_battle {
        session.paused = false;
    }
    let asset_path = locate_assets()?;

    let mut app = App::new();
    app.insert_resource(ClearColor(Color::srgb(0.52, 0.64, 0.68)))
        .insert_resource(ScenarioDraft(config))
        .insert_resource(session)
        .insert_resource(options.clone())
        .add_plugins(
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "Legion — Battle Lab".into(),
                        resolution: (options.width, options.height).into(),
                        present_mode: PresentMode::AutoVsync,
                        resize_constraints: bevy::window::WindowResizeConstraints {
                            min_width: 1024.0,
                            min_height: 768.0,
                            ..default()
                        },
                        ..default()
                    }),
                    ..default()
                })
                .set(AssetPlugin {
                    file_path: asset_path,
                    ..default()
                }),
        )
        .init_state::<Screen>()
        .add_message::<AppAction>()
        .configure_sets(
            Update,
            (
                AppSet::Input,
                AppSet::Actions,
                AppSet::Simulation,
                AppSet::Presentation,
            )
                .chain(),
        )
        .add_systems(Update, keyboard_actions.in_set(AppSet::Input))
        .add_systems(Update, apply_actions.in_set(AppSet::Actions))
        .add_systems(Update, advance_simulation.in_set(AppSet::Simulation))
        .add_plugins((
            crate::crowd::CrowdRenderPlugin,
            crate::world::BattleWorldPlugin,
            crate::camera::BattleCameraPlugin,
            crate::ui::GameUiPlugin,
            crate::smoke::SmokeTestPlugin,
        ));
    if start_battle {
        app.world_mut()
            .resource_mut::<NextState<Screen>>()
            .set(Screen::Battle);
    }
    if app.run().is_error() {
        Err(
            "Der Laufzeittest hat einen Fehler gemeldet. Siehe Protokoll und smoke-report.json."
                .into(),
        )
    } else {
        Ok(())
    }
}

fn locate_assets() -> Result<String, String> {
    let mut candidates = vec![PathBuf::from("assets")];
    if let Ok(exe) = std::env::current_exe()
        && let Some(parent) = exe.parent()
    {
        candidates.push(parent.join("assets"));
    }
    candidates.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets"));
    let required = [
        "shaders/crowd.wesl",
        "fonts/Barlow-Regular.ttf",
        "fonts/Barlow-SemiBold.ttf",
    ];
    candidates.into_iter().find(|p| required.iter().all(|file| p.join(file).is_file()))
        .and_then(|p| p.canonicalize().ok())
        .map(|p| p.to_string_lossy().into_owned())
        .ok_or_else(|| "Benötigte Assets fehlen: assets/shaders/crowd.wesl und beide Barlow-Schriften in assets/fonts. Starte im vollständig entpackten Projektordner oder lege den vollständigen assets-Ordner neben die ausführbare Datei.".into())
}

fn keyboard_actions(
    keys: Res<ButtonInput<KeyCode>>,
    screen: Res<State<Screen>>,
    focus: Res<InputFocus>,
    ui_nodes: Query<(), With<Node>>,
    session: Res<Session>,
    mut actions: MessageWriter<AppAction>,
) {
    if keys.just_pressed(KeyCode::F1) {
        actions.write(AppAction::ToggleHelp);
    }
    if keys.just_pressed(KeyCode::F3) {
        actions.write(AppAction::ToggleDiagnostics);
    }
    if keys.just_pressed(KeyCode::Escape) {
        actions.write(if session.help {
            AppAction::ToggleHelp
        } else {
            match screen.get() {
                Screen::Menu => AppAction::ToggleHelp,
                Screen::Configure => AppAction::MainMenu,
                Screen::Battle => AppAction::TogglePause,
            }
        });
    }
    let ui_has_focus = focus.get().is_some_and(|entity| ui_nodes.contains(entity));
    if *screen.get() != Screen::Battle || session.help || ui_has_focus {
        return;
    }
    if keys.just_pressed(KeyCode::Space) {
        actions.write(AppAction::TogglePause);
    }
    if keys.just_pressed(KeyCode::Period) {
        actions.write(AppAction::Step);
    }
    for (key, speed) in [
        (KeyCode::Digit1, 0.5),
        (KeyCode::Digit2, 1.0),
        (KeyCode::Digit3, 2.0),
        (KeyCode::Digit4, 4.0),
    ] {
        if keys.just_pressed(key) {
            actions.write(AppAction::SetSpeed(speed));
        }
    }
}

fn army_mut(config: &mut ScenarioConfig, team: Team) -> &mut ArmyConfig {
    match team {
        Team::Blue => &mut config.blue,
        Team::Red => &mut config.red,
    }
}

fn apply_actions(
    mut actions: MessageReader<AppAction>,
    mut draft: ResMut<ScenarioDraft>,
    mut session: ResMut<Session>,
    screen: Res<State<Screen>>,
    mut next: ResMut<NextState<Screen>>,
    options: Res<LaunchOptions>,
    mut exit: MessageWriter<AppExit>,
) {
    for action in actions.read() {
        match *action {
            AppAction::Configure => {
                session.set_paused(true);
                session.help = false;
                session.status.clear();
                next.set(Screen::Configure);
            }
            AppAction::QuickBattle => match session.reset(draft.0.clone(), false) {
                Ok(()) => next.set(Screen::Battle),
                Err(e) => session.status = e,
            },
            AppAction::StartBattle => match session.reset(draft.0.clone(), false) {
                Ok(()) => {
                    session.help = false;
                    next.set(Screen::Battle);
                }
                Err(e) => session.status = e,
            },
            AppAction::MainMenu => {
                session.help = false;
                session.set_paused(true);
                next.set(Screen::Menu);
                if let Err(error) = session.reset(draft.0.clone(), true) {
                    session.status = error;
                }
            }
            AppAction::TogglePause => {
                if *screen.get() == Screen::Battle
                    && session.is_ready()
                    && session.battle.as_ref().is_some_and(|b| b.outcome.is_none())
                {
                    let paused = !session.paused;
                    session.set_paused(paused);
                }
            }
            AppAction::Step => {
                if *screen.get() == Screen::Battle
                    && session.paused
                    && !session.busy
                    && session.is_ready()
                    && session
                        .battle
                        .as_ref()
                        .is_some_and(|battle| battle.outcome.is_none())
                {
                    session.single_step = true;
                }
            }
            AppAction::Restart => {
                let config = session
                    .battle
                    .as_ref()
                    .map(|b| b.config.clone())
                    .unwrap_or_else(|| draft.0.clone());
                match session.reset(config, false) {
                    Ok(()) => next.set(Screen::Battle),
                    Err(e) => session.status = e,
                }
            }
            AppAction::SetSpeed(speed) => {
                if *screen.get() == Screen::Battle
                    && session.is_ready()
                    && session.battle.as_ref().is_some_and(|b| b.outcome.is_none())
                    && [0.5, 1.0, 2.0, 4.0].contains(&speed)
                {
                    session.speed = speed;
                }
            }
            AppAction::ToggleDiagnostics => session.diagnostics = !session.diagnostics,
            AppAction::ToggleHelp => {
                if !session.help && *screen.get() == Screen::Battle {
                    session.set_paused(true);
                }
                session.help = !session.help;
            }
            AppAction::Quit => {
                exit.write(AppExit::Success);
            }
            AppAction::Preset(index) => {
                draft.0 = ScenarioConfig::preset(index);
            }
            AppAction::SetMap(map) => {
                draft.0.map = map;
            }
            AppAction::SetCategory(team, category) => {
                army_mut(&mut draft.0, team).category = category;
            }
            AppAction::SetKind(team, kind) => {
                army_mut(&mut draft.0, team).kind = kind;
            }
            AppAction::SetFormation(team, formation) => {
                army_mut(&mut draft.0, team).formation = formation;
            }
            AppAction::ChangeCount(team, delta) => {
                let other = match team {
                    Team::Blue => draft.0.red.count,
                    Team::Red => draft.0.blue.count,
                };
                let limit = battle_core::MAX_UNITS.saturating_sub(other).max(1);
                let army = army_mut(&mut draft.0, team);
                army.count =
                    (i64::from(army.count) + i64::from(delta)).clamp(1, i64::from(limit)) as u32;
            }
            AppAction::ChangeSeed(delta) => {
                draft.0.seed = if delta < 0 {
                    draft.0.seed.saturating_sub(delta.unsigned_abs())
                } else {
                    draft.0.seed.saturating_add(delta as u64)
                };
            }
            AppAction::ChangeDuration(delta) => {
                draft.0.time_limit_secs =
                    (i64::from(draft.0.time_limit_secs) + i64::from(delta)).clamp(30, 3600) as u32;
            }
            AppAction::SaveScenario => {
                let path = options.data_dir.join("scenario.json");
                session.status = match write_scenario(&path, &draft.0) {
                    Ok(()) => format!("Szenario gespeichert: {}", path.display()),
                    Err(e) => e,
                };
            }
            AppAction::LoadScenario => {
                let path = options.data_dir.join("scenario.json");
                match read_scenario(&path) {
                    Ok(config) => {
                        draft.0 = config;
                        session.status = "Gespeichertes Szenario geladen.".into();
                    }
                    Err(e) => session.status = e,
                }
            }
        }
    }
}

fn advance_simulation(
    time: Res<Time<Real>>,
    screen: Res<State<Screen>>,
    mut session: ResMut<Session>,
) {
    let wall_delta = time.delta_secs_f64();
    session.last_frame_ms = (wall_delta * 1000.0) as f32;
    if !session.paused && !session.preparing {
        session.presentation_age += wall_delta as f32;
        session.publication_elapsed += wall_delta as f32;
    }
    if wall_delta > 0.0 {
        let weight = (wall_delta * 2.0).clamp(0.0, 1.0) as f32;
        session.smoothed_fps += ((1.0 / wall_delta) as f32 - session.smoothed_fps) * weight;
    }
    session.receive_worker();
    if *screen.get() != Screen::Battle || !session.is_ready() {
        return;
    }
    if session.single_step && !session.busy {
        session.single_step = false;
        session.accumulator = 0.0;
        if let Err(error) = session.submit_ticks(1) {
            session.status = error;
        }
        return;
    }
    if session.paused {
        session.presentation_age = session.presentation_interval;
        return;
    }
    let accepted_delta = wall_delta.min(0.25);
    session.dropped_time += (wall_delta - accepted_delta) * f64::from(session.speed);
    session.accumulator += accepted_delta * f64::from(session.speed);
    if session.accumulator > 0.4 {
        session.dropped_time += session.accumulator - 0.4;
        session.accumulator = 0.4;
    }
    if session.busy {
        return;
    }
    let due = ((session.accumulator + 1e-9) / (1.0 / SIM_HZ)).floor() as u32;
    // Large worlds publish at most10 snapshots/second at1x. The simulation
    // still executes every fixed50ms tick; snapshots cover both completed ticks.
    let minimum_batch = if session
        .battle
        .as_ref()
        .is_some_and(|b| b.units.len() >= 100_000)
    {
        2
    } else {
        1
    };
    if due < minimum_batch {
        return;
    }
    let steps = due.min(8);
    match session.submit_ticks(steps) {
        Ok(()) => session.accumulator -= f64::from(steps) * (1.0 / SIM_HZ),
        Err(error) => {
            session.status = error;
            session.paused = true;
        }
    }
}

const MAX_SCENARIO_BYTES: u64 = 64 * 1024;
static SCENARIO_TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

fn validate_scenario_file(metadata: &fs::Metadata) -> Result<(), String> {
    if !metadata.is_file() {
        return Err("Ein Szenario muss eine reguläre Datei sein.".into());
    }
    if metadata.len() > MAX_SCENARIO_BYTES {
        return Err("Szenariodatei ist zu groß (maximal 64 KiB).".into());
    }
    Ok(())
}

pub fn read_scenario(path: &Path) -> Result<ScenarioConfig, String> {
    let metadata = fs::metadata(path).map_err(|e| {
        format!(
            "Szenario konnte nicht geladen werden ({}): {e}",
            path.display()
        )
    })?;
    // Reject devices and named pipes before opening, which could otherwise
    // block the UI or produce an unlimited stream despite a zero file length.
    validate_scenario_file(&metadata)?;
    let file = fs::File::open(path).map_err(|e| format!("Szenario nicht lesbar: {e}"))?;
    let opened_metadata = file
        .metadata()
        .map_err(|e| format!("Szenariodatei nicht prüfbar: {e}"))?;
    validate_scenario_file(&opened_metadata)?;
    read_scenario_contents(file)
}

fn read_scenario_contents(reader: impl Read) -> Result<ScenarioConfig, String> {
    // Bound the actual read too: a regular file can grow after metadata is read.
    let mut bytes = Vec::new();
    reader
        .take(MAX_SCENARIO_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("Szenario nicht lesbar: {e}"))?;
    if bytes.len() as u64 > MAX_SCENARIO_BYTES {
        return Err("Szenariodatei ist zu groß (maximal 64 KiB).".into());
    }
    let text = std::str::from_utf8(&bytes)
        .map_err(|e| format!("Szenario enthält kein gültiges UTF-8: {e}"))?;
    ScenarioConfig::from_json(text)
}

pub fn write_scenario(path: &Path, config: &ScenarioConfig) -> Result<(), String> {
    config.validate()?;
    let json = serde_json::to_vec_pretty(config).map_err(|e| e.to_string())?;
    let (temporary, mut file) = create_scenario_temporary(path)?;
    let written = file
        .write_all(&json)
        .and_then(|()| file.sync_all())
        .map_err(|e| format!("Speichern fehlgeschlagen: {e}"));
    // Closing before replacement also permits the rename on Windows.
    drop(file);
    let result = written.and_then(|()| {
        fs::rename(&temporary, path)
            .map_err(|e| format!("Szenariodatei konnte nicht ersetzt werden: {e}"))
    });
    if result.is_err() {
        // Never delete the previous scenario to make a failed replacement work.
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn create_scenario_temporary(path: &Path) -> Result<(PathBuf, fs::File), String> {
    let filename = path
        .file_name()
        .ok_or("Der Szenariopfad benötigt einen Dateinamen.")?;
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|e| format!("Speicherordner nicht verfügbar: {e}"))?;
    for _ in 0..128 {
        let sequence = SCENARIO_TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let mut temporary_name = std::ffi::OsString::from(".");
        temporary_name.push(filename);
        temporary_name.push(format!(".{}.{sequence}.tmp", std::process::id()));
        let temporary = parent.join(temporary_name);
        // Exclusive creation cannot truncate another writer or follow a stale
        // temporary symlink. A crash leftover merely makes us try another name.
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
        {
            Ok(file) => return Ok((temporary, file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(format!("Szenario nicht speicherbar: {error}")),
        }
    }
    Err("Keine freie temporäre Datei zum Speichern gefunden.".into())
}

#[cfg(test)]
mod persistence_tests {
    use super::*;

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);
            loop {
                let sequence = NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed);
                let path = std::env::temp_dir().join(format!(
                    "legion-scenario-io-{}-{sequence}",
                    std::process::id()
                ));
                match fs::create_dir(&path) {
                    Ok(()) => return Self(path),
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                    Err(error) => panic!("Cannot create test directory: {error}"),
                }
            }
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn repeated_save_replaces_complete_scenario_without_temporary_files() {
        let directory = TestDirectory::new();
        let path = directory.0.join("nested/scenario.json");
        let mut config = ScenarioConfig::default();
        write_scenario(&path, &config).unwrap();
        config.seed = u64::MAX;
        config.blue.category = UnitCategory::SpaceUnit;
        write_scenario(&path, &config).unwrap();
        assert_eq!(read_scenario(&path).unwrap(), config);
        assert_eq!(fs::read_dir(path.parent().unwrap()).unwrap().count(), 1);
    }

    #[test]
    fn invalid_save_preserves_previous_scenario() {
        let directory = TestDirectory::new();
        let path = directory.0.join("scenario.json");
        let mut config = ScenarioConfig::default();
        write_scenario(&path, &config).unwrap();
        let previous = fs::read(&path).unwrap();
        config.blue.count = u32::MAX;
        assert!(write_scenario(&path, &config).is_err());
        assert_eq!(fs::read(&path).unwrap(), previous);
        assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 1);
    }

    #[test]
    fn failed_replacement_preserves_target_and_removes_temporary_file() {
        let directory = TestDirectory::new();
        let path = directory.0.join("scenario.json");
        fs::create_dir(&path).unwrap();
        let sentinel = path.join("keep.txt");
        fs::write(&sentinel, b"previous data").unwrap();
        assert!(write_scenario(&path, &ScenarioConfig::default()).is_err());
        assert_eq!(fs::read(&sentinel).unwrap(), b"previous data");
        assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 1);
    }

    #[test]
    fn loading_rejects_oversized_invalid_utf8_and_unknown_fields() {
        let directory = TestDirectory::new();
        let path = directory.0.join("scenario.json");
        fs::write(&path, vec![b' '; MAX_SCENARIO_BYTES as usize + 1]).unwrap();
        assert!(read_scenario(&path).unwrap_err().contains("64 KiB"));
        fs::write(&path, [0xff]).unwrap();
        assert!(read_scenario(&path).unwrap_err().contains("UTF-8"));
        let config = serde_json::to_value(ScenarioConfig::default()).unwrap();
        let mut unknown_root = config.clone();
        unknown_root["unexpected"] = true.into();
        let mut unknown_army = config;
        unknown_army["blue"]["unexpected"] = true.into();
        for invalid in [unknown_root, unknown_army] {
            fs::write(&path, serde_json::to_vec(&invalid).unwrap()).unwrap();
            assert!(read_scenario(&path).unwrap_err().contains("unknown field"));
        }
    }

    #[test]
    fn actual_read_is_bounded_even_for_an_unending_source() {
        struct UnendingReader {
            bytes_read: usize,
        }
        impl Read for UnendingReader {
            fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
                buffer.fill(b' ');
                self.bytes_read += buffer.len();
                Ok(buffer.len())
            }
        }
        let mut reader = UnendingReader { bytes_read: 0 };
        assert!(
            read_scenario_contents(&mut reader)
                .unwrap_err()
                .contains("64 KiB")
        );
        assert_eq!(reader.bytes_read, MAX_SCENARIO_BYTES as usize + 1);
    }

    #[test]
    fn concurrent_saves_leave_one_complete_scenario() {
        let directory = TestDirectory::new();
        let path = directory.0.join("scenario.json");
        let barrier = std::sync::Barrier::new(8);
        std::thread::scope(|scope| {
            for seed in 0..8 {
                let path = &path;
                let barrier = &barrier;
                scope.spawn(move || {
                    let config = ScenarioConfig {
                        seed,
                        ..Default::default()
                    };
                    barrier.wait();
                    for _ in 0..4 {
                        write_scenario(path, &config).unwrap();
                    }
                });
            }
        });
        let saved = read_scenario(&path).unwrap();
        assert!(saved.seed < 8);
        assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 1);
    }

    #[test]
    fn loading_rejects_a_directory_before_reading() {
        let directory = TestDirectory::new();
        assert!(
            read_scenario(&directory.0)
                .unwrap_err()
                .contains("reguläre Datei")
        );
    }

    #[cfg(unix)]
    #[test]
    fn loading_rejects_a_special_file_without_reading_it() {
        assert!(
            read_scenario(Path::new("/dev/null"))
                .unwrap_err()
                .contains("reguläre Datei")
        );
    }

    #[cfg(unix)]
    #[test]
    fn saving_does_not_follow_a_legacy_temporary_symlink() {
        let directory = TestDirectory::new();
        let path = directory.0.join("scenario.json");
        let sentinel = directory.0.join("keep.txt");
        fs::write(&sentinel, b"previous data").unwrap();
        std::os::unix::fs::symlink(&sentinel, path.with_extension("json.tmp")).unwrap();
        let config = ScenarioConfig::default();
        write_scenario(&path, &config).unwrap();
        assert_eq!(fs::read(&sentinel).unwrap(), b"previous data");
        assert_eq!(read_scenario(&path).unwrap(), config);
    }
}
