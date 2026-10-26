//! [`GameBuilder`]: content, sim and client wired into one Bevy `App` (spec §5.2, §5.3, §15;
//! ADR-0001, ADR-0002).
//!
//! The sim runs on its own thread ([`SimRunner::spawn_threaded`]); the client drives it through
//! a [`LocalDriver`] and the [`RunnerLink`], and draws its snapshots.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use bevy::log::tracing_subscriber::layer::Identity;
use bevy::log::{BoxedFmtLayer, BoxedLayer, DEFAULT_FILTER, LogPlugin};
use bevy::picking::mesh_picking::MeshPickingPlugin;
use bevy::prelude::*;
use bevy::window::PresentMode;
use bevy::winit::{WinitPlugin, WinitSettings};
use fl_client::ai::AiHost;
use fl_client::debug::{
    CurrentScenario, DebugCommandsPlugin, LoadHook, LoadRequest, LoadedMatch, ProfilerSource,
    ScenarioLoader,
};
use fl_client::link::SimJournalMsg;
use fl_client::map_view::MapView;
use fl_client::placeholder::PlayerColors;
use fl_client::view::{ClientContent, LocalPlayer};
use fl_client::{
    AiPlugin, ClientFeature, ClientPlugins, LocalDriver, PlaceholderPlugin, RunnerLink,
    SimViewPlugin,
};
use fl_content::{ContentDb, Faction, ScenarioDef};
use fl_observe::{
    DebugConfig, JournalSink, Profiler, RunDir, global_profiler, global_run_dir, load_debug_config,
    new_run_dir, observe_layers, set_global_run_dir,
};
use fl_protocol::{Controller, PlayerCommand, PlayerId, SimId, Tick};
use fl_sim::{
    CoreFeature, LoadOptions, MatchHeaderRes, MatchSetup, Players, ReplayStart, RunConfig,
    ScaleScenario, SimFeature, SimRunner, build_sim_app, load_content, load_world, load_world_with,
    setup_scenario, sort_features,
};
use fl_view::ai_controller;

use crate::reload::{ContentSource, ContentWatcher, MatchTerrain};
use crate::replay_capture::ReplayCapture;

/// The testbed's window title.
const TITLE: &str = "Frontline — testbed";

/// Frames in the first seconds of an `exit_after` run do not count toward its FPS figures
/// (shader compilation, asset loading).
const FPS_WARMUP: Duration = Duration::from_secs(3);

/// What the match starts from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StartMode {
    /// A scenario of the loaded content, by id.
    Scenario(String),
    /// The [`ScaleScenario`] entity mix scaled to `units` wandering units on the 2 km terrain
    /// map ([`ScaleScenario::on_terrain`]: followers and projectiles in flight), placed with
    /// `seed`.
    Stress { units: u32, seed: u64 },
}

/// Builds the testbed: content, the threaded sim, the client and its observability.
pub struct GameBuilder {
    start: StartMode,
    sim_features: Vec<Arc<dyn SimFeature>>,
    client_features: Vec<Arc<dyn ClientFeature>>,
    /// `None`: the base mod.
    mods: Option<Vec<PathBuf>>,
    exit_after: Option<f32>,
    debug_remote: Option<u16>,
    agent_mode: bool,
    hidden_window: bool,
    /// Whether the match allows cheats (`debug.*` commands, hot reload).
    cheats: bool,
    /// Where a windowed debug run creates its run directory.
    logs_dir: PathBuf,
    /// The `debug.ron` a windowed run reads; `None`: `./debug.ron` if it exists.
    debug_ron: Option<PathBuf>,
}

impl Default for GameBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Which client the builder makes.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Shell {
    /// `DefaultPlugins`, a window, logging and the debug UI.
    Windowed,
    /// `MinimalPlugins` and the headless client plugins (tests).
    Headless,
}

/// The running sim and what the client needs to know about it.
struct SimStart {
    /// The client features in feature order.
    client_features: Vec<Arc<dyn ClientFeature>>,
    link: RunnerLink,
    next_tick: Tick,
    /// Starts later matches (`fl.load_scenario`, `fl.load`).
    loader: ScenarioLoader,
    /// What hot reloads load the content from.
    source: ContentSource,
    content: Arc<ContentDb>,
    map: MapView,
    terrain: MatchTerrain,
    colors: PlayerColors,
    /// The AI controllers of the scenario's `Controller::Ai` players.
    ai: AiHost,
    capture: ReplayCapture,
    units: usize,
}

impl GameBuilder {
    /// The testbed defaults: scenario `base.testbed`, the [`fl_catalog::features`], the base
    /// mod, cheats on.
    #[must_use]
    pub fn new() -> Self {
        GameBuilder {
            start: StartMode::Scenario("base.testbed".to_owned()),
            sim_features: fl_catalog::features(),
            client_features: Vec::new(),
            mods: None,
            exit_after: None,
            debug_remote: None,
            agent_mode: false,
            hidden_window: false,
            cheats: true,
            logs_dir: PathBuf::from("logs"),
            debug_ron: None,
        }
    }

    /// Replaces the sim features (the core feature is always included).
    #[must_use]
    pub fn sim_features(mut self, fs: Vec<Arc<dyn SimFeature>>) -> Self {
        self.sim_features = fs;
        self
    }

    /// Adds a client feature; its sim feature must be in the match.
    #[must_use]
    pub fn client_feature(mut self, f: impl ClientFeature) -> Self {
        self.client_features.push(Arc::new(f));
        self
    }

    /// Adds a mod directory, in load order. The first call replaces the default base mod.
    #[must_use]
    pub fn content_mod(mut self, dir: impl Into<PathBuf>) -> Self {
        self.mods.get_or_insert_with(Vec::new).push(dir.into());
        self
    }

    /// Starts scenario `id`.
    #[must_use]
    pub fn scenario(mut self, id: &str) -> Self {
        self.start = StartMode::Scenario(id.to_owned());
        self
    }

    /// Starts the [`ScaleScenario`] entity mix scaled to `units` wandering units on the 2 km
    /// terrain map ([`ScaleScenario::on_terrain`]) with its default seed.
    #[must_use]
    pub fn stress(mut self, units: u32) -> Self {
        self.start = StartMode::Stress {
            units,
            seed: ScaleScenario::default().seed,
        };
        self
    }

    /// Exits after `secs` seconds and prints the FPS line (see [`GameBuilder::prepare`]).
    #[must_use]
    pub fn exit_after(mut self, secs: Option<f32>) -> Self {
        self.exit_after = secs;
        self
    }

    /// Serves the remote debug endpoint (BRP `fl.*` methods) on this port; `None` for none.
    /// Needs the `debug_remote` feature and a debug build; otherwise it only logs.
    #[must_use]
    pub fn debug_remote(mut self, port: Option<u16>) -> Self {
        self.debug_remote = port;
        self
    }

    /// Keeps updating continuously while the window is unfocused (benches and agents).
    #[must_use]
    pub fn agent_mode(mut self, on: bool) -> Self {
        self.agent_mode = on;
        self
    }

    /// Whether the match allows cheats (default true: the testbed's debug console, spawn
    /// panel and hot reload need them).
    #[must_use]
    pub fn cheats(mut self, on: bool) -> Self {
        self.cheats = on;
        self
    }

    /// Hides the window and lets the event loop run on any thread (Windows and Linux): for GPU
    /// tests, which run off the main thread. Windowed mode only.
    #[must_use]
    pub fn hidden_window(mut self, on: bool) -> Self {
        self.hidden_window = on;
        self
    }

    /// The directory a windowed debug run creates its run directory `<run-id>/` in; default
    /// `logs` (relative to the working directory).
    #[must_use]
    pub fn logs_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.logs_dir = dir.into();
        self
    }

    /// The `debug.ron` a windowed run reads ([`load_debug_config`]); default `./debug.ron` if it
    /// exists, else the built-in defaults.
    #[must_use]
    pub fn debug_ron(mut self, path: impl Into<PathBuf>) -> Self {
        self.debug_ron = Some(path.into());
        self
    }

    /// Everything except `run()`:
    /// 1. loads `debug.ron` ([`load_debug_config`]), the content, and checks the client features
    ///    ([`check_client_features`]);
    /// 2. builds the sim (cheats allowed unless [`GameBuilder::cheats`] turns them off; with
    ///    the phase markers when `debug.ron` turns the profiler on), sets up the scenario or
    ///    the stress units and starts the sim thread;
    /// 3. builds the client `App`: `DefaultPlugins` (window title, no vsync in stress mode,
    ///    continuous updates in agent mode, logging from `debug.ron`), `MeshPickingPlugin`,
    ///    [`ClientPlugins`], then the client features in feature order; the [`RunnerLink`],
    ///    [`LocalDriver`], [`ClientContent`], [`LocalPlayer`] 0, [`MapView`] and
    ///    [`PlayerColors`]; the [`ScenarioLoader`], [`CurrentScenario`] and [`ProfilerSource`];
    ///    with [`GameBuilder::debug_remote`], the remote debug endpoint;
    /// 4. adds hot reload ([`crate::reload`]: the [`ContentSource`], the [`MatchTerrain`] the
    ///    map view follows, the console command `reload`, and in a windowed dev build with
    ///    `debug.ron` `hot_reload` the [`ContentWatcher`]) and the replay capture
    ///    ([`ReplayCapture`], the client's `InputLog`, the console command `replay save`).
    ///
    /// Observability: in debug builds a run directory `<logs_dir>/<run-id>/` receives `log.jsonl`
    /// and `journal.jsonl`; the profiler layer times sim ticks and client frames.
    ///
    /// With [`GameBuilder::exit_after`], the app exits after that many seconds and prints
    /// `stress: units={n} avg_fps={:.1} p1_low_fps={:.1} frames={}`, ignoring the first 3 s.
    ///
    /// # Errors
    /// Content, feature and setup errors, as text, before any window or sim thread exists.
    pub fn prepare(self) -> Result<App, String> {
        self.build(Shell::Windowed)
    }

    /// [`GameBuilder::prepare`] with `MinimalPlugins` and the headless client plugins: no
    /// window, no logging setup, no run directory (tests).
    ///
    /// # Errors
    /// As [`GameBuilder::prepare`].
    pub fn prepare_headless(self) -> Result<App, String> {
        self.build(Shell::Headless)
    }

    /// [`GameBuilder::prepare`], then runs the app. On an error, prints it and returns
    /// [`AppExit::error`].
    pub fn run(self) -> AppExit {
        match self.prepare() {
            Ok(mut app) => app.run(),
            Err(error) => {
                for line in error.lines() {
                    eprintln!("{line}");
                }
                AppExit::error()
            }
        }
    }

    fn build(self, shell: Shell) -> Result<App, String> {
        // `debug.ron` comes first: it decides whether the sim emits the phase markers the
        // profiler times phases by.
        let (cfg, run) = self.startup_config(shell);
        let sim = self.start_sim(&run)?;
        // Automatic hot reload is a dev-build testbed tool (ADR-0012).
        let watcher = cfg
            .as_ref()
            .filter(|cfg| cfg.hot_reload && cfg!(debug_assertions))
            .map(|cfg| {
                ContentWatcher::new(
                    sim.source.mods.clone(),
                    Duration::from_millis(cfg.hot_reload_poll_ms),
                )
            });
        // Only now, so that a content error leaves no empty run directory behind.
        if let Some(cfg) = &cfg {
            observe_setup(&self.logs_dir, cfg);
        }
        let mut app = App::new();
        // `cfg` is `Some` exactly for the windowed shell.
        match cfg {
            Some(cfg) => {
                app.insert_resource(ObserveConfig(cfg.clone()));
                let mut window = Window {
                    title: TITLE.to_owned(),
                    ..default()
                };
                if matches!(self.start, StartMode::Stress { .. }) {
                    // Uncapped, so the FPS line measures the frame cost.
                    window.present_mode = PresentMode::AutoNoVsync;
                }
                window.visible = !self.hidden_window;
                app.add_plugins(
                    DefaultPlugins
                        .set(WindowPlugin {
                            primary_window: Some(window),
                            ..default()
                        })
                        .set(LogPlugin {
                            filter: log_plugin_filter(&cfg),
                            custom_layer: client_layers,
                            fmt_layer: no_fmt_layer,
                            ..default()
                        })
                        .set(WinitPlugin {
                            run_on_any_thread: self.hidden_window,
                        }),
                )
                .add_plugins((MeshPickingPlugin, ClientPlugins));
                if self.agent_mode {
                    app.insert_resource(WinitSettings::continuous());
                }
                if cfg.journal_file
                    && let Some(run) = global_run_dir()
                {
                    match JournalSink::create(run) {
                        Ok(sink) => {
                            app.insert_resource(JournalFile(sink));
                        }
                        Err(err) => tracing::warn!(%err, "no journal.jsonl"),
                    }
                }
            }
            None => {
                app.add_plugins((
                    MinimalPlugins,
                    SimViewPlugin,
                    PlaceholderPlugin,
                    DebugCommandsPlugin,
                    AiPlugin,
                ));
            }
        }
        if let Some(port) = self.debug_remote {
            add_remote_debug(&mut app, port);
        }
        for feature in &sim.client_features {
            feature.build(&mut app);
        }

        crate::reload::build(&mut app, watcher);
        crate::replay_capture::build(&mut app);
        app.insert_resource(sim.link)
            .insert_resource(LocalDriver::new(sim.next_tick))
            .insert_resource(ClientContent(sim.content))
            .insert_resource(LocalPlayer(PlayerId(0)))
            .insert_resource(sim.map)
            .insert_resource(sim.terrain)
            .insert_resource(sim.colors)
            .insert_resource(sim.ai)
            .insert_resource(sim.capture)
            .insert_resource(sim.loader)
            .insert_resource(sim.source)
            .insert_resource(CurrentScenario(match &self.start {
                StartMode::Scenario(id) => Some(id.clone()),
                StartMode::Stress { .. } => None,
            }))
            .insert_resource(ProfilerSource(profiler_json))
            .add_systems(
                PostUpdate,
                write_journal.run_if(resource_exists::<JournalFile>),
            )
            .add_systems(Last, close_frame_span);
        if let Some(secs) = self.exit_after {
            app.insert_resource(ExitAfter::new(secs, sim.units))
                .add_systems(Last, exit_after);
        }
        Ok(app)
    }

    /// What `debug.ron` (windowed shell only) and the shell decide before the sim starts: the
    /// debug config, and the sim's [`RunConfig`] with the phase markers on exactly when the
    /// profiler is.
    fn startup_config(&self, shell: Shell) -> (Option<DebugConfig>, RunConfig) {
        let cfg = match shell {
            Shell::Windowed => Some(load_debug_config(self.debug_ron.as_deref())),
            Shell::Headless => None,
        };
        let run = RunConfig {
            phase_markers: cfg.as_ref().is_some_and(|cfg| cfg.profiler),
            ..RunConfig::default()
        };
        (cfg, run)
    }

    /// Loads the content, builds and sets up the sim with `run`, and starts its thread.
    fn start_sim(&self, run: &RunConfig) -> Result<SimStart, String> {
        let mods = self
            .mods
            .clone()
            .unwrap_or_else(|| vec![fl_catalog::base_mod_dir()]);
        let content = load_content(&self.sim_features, &mods).map_err(|e| e.to_string())?;
        check_client_features(&self.sim_features, &self.client_features)?;
        let client_features = sorted_client_features(&self.sim_features, &self.client_features)?;

        let sim = set_up_sim(&self.sim_features, &content, &self.start, run, self.cheats)?;
        let world = sim.app.world();
        let terrain = MatchTerrain::of_sim(world, &content, sim.map.as_deref());
        let map = terrain.map_view(&content);
        let colors = player_colors(&content, world.resource::<Players>());
        let ai = ai_host(world.resource::<Players>())?;
        let capture = ReplayCapture::of_sim(world, sim.start);
        let units = sim.units;
        let (link, next_tick) = run_sim(sim.app);
        let source = ContentSource::new(self.sim_features.clone(), mods, run.clone(), self.cheats);
        Ok(SimStart {
            client_features,
            link,
            next_tick,
            loader: scenario_loader(&source, content.clone()),
            source,
            content,
            map,
            terrain,
            colors,
            ai,
            capture,
            units,
        })
    }
}

/// The [`AiHost`] of `players`: a controller for each `Controller::Ai(id)` player.
///
/// # Errors
/// `unknown AI controller '<id>' for player <n>` when no controller has that id.
fn ai_host(players: &Players) -> Result<AiHost, String> {
    let mut host = AiHost::default();
    for player in &players.list {
        if let Controller::Ai(id) = &player.controller {
            let controller = ai_controller(id).ok_or_else(|| {
                format!("unknown AI controller '{id}' for player {}", player.id.0)
            })?;
            host.players.insert(player.id, (player.team, controller));
        }
    }
    Ok(host)
}

/// A sim built and set up, not yet running.
struct SetUpSim {
    app: App,
    /// The units placed.
    units: usize,
    /// The map's id.
    map: Option<String>,
    /// How a replay sets the match up.
    start: ReplayStart,
}

/// Builds the sim of `features` with `run` and sets up `start`: the scenario, or the stress
/// units. The match allows cheats when `cheats` is set (the testbed: the debug console, spawn
/// panel and hot reload).
fn set_up_sim(
    features: &[Arc<dyn SimFeature>],
    content: &Arc<ContentDb>,
    start: &StartMode,
    run: &RunConfig,
    cheats: bool,
) -> Result<SetUpSim, String> {
    let setup = match start {
        StartMode::Scenario(id) => MatchSetup {
            cheats_allowed: cheats,
            ..MatchSetup::for_scenario(content, id).map_err(|e| e.to_string())?
        },
        StartMode::Stress { seed, .. } => MatchSetup {
            seed: *seed,
            cheats_allowed: cheats,
        },
    };
    let mut app =
        build_sim_app(features, content.clone(), setup, run.clone()).map_err(|e| e.to_string())?;

    let world = app.world_mut();
    let (units, map, start) = match start {
        StartMode::Scenario(id) => {
            let units = setup_scenario(world, id).map_err(|e| e.to_string())?;
            let map = content
                .kind::<ScenarioDef>()
                .get(id)
                .map(|def| def.map.clone());
            (units.len(), map, ReplayStart::Scenario(id.clone()))
        }
        StartMode::Stress { units, seed } => {
            let scale = ScaleScenario {
                seed: *seed,
                ..ScaleScenario::on_terrain(*units)
            };
            let units = scale.populate(world).map_err(|e| e.to_string())?;
            (units.len(), scale.map.clone(), ReplayStart::scale(&scale))
        }
    };
    Ok(SetUpSim {
        app,
        units,
        map,
        start,
    })
}

/// Starts the sim of `app` on its own thread; returns the link to it and its first tick.
fn run_sim(app: App) -> (RunnerLink, Tick) {
    let runner = SimRunner::new(app);
    let next_tick = runner.next_tick();
    let header = runner.world().resource::<MatchHeaderRes>().0.clone();
    // The thread ends when the client drops its port (the app exits, or a load replaces it).
    let (port, _thread) = runner.spawn_threaded();
    (RunnerLink { port, header }, next_tick)
}

/// Starts scenarios, saves and hot-reload rebuilds with the features, run config and cheats
/// flag of `source`; scenarios and saves load under `content`, a rebuild under its own.
pub(crate) fn scenario_loader(source: &ContentSource, content: Arc<ContentDb>) -> ScenarioLoader {
    let source = source.clone();
    ScenarioLoader(Box::new(move |request| match request {
        LoadRequest::Scenario(id) => {
            let start = StartMode::Scenario(id);
            let sim = set_up_sim(
                &source.features,
                &content,
                &start,
                &source.run,
                source.cheats,
            )?;
            let started = Started {
                map: sim.map,
                start: Some(sim.start),
                pending: Vec::new(),
                last_seq: BTreeMap::new(),
                dropped: Vec::new(),
            };
            started.into_match(sim.app, &content)
        }
        LoadRequest::Save(bytes) => {
            let loaded = load_world(
                &source.features,
                content.clone(),
                &bytes,
                source.run.clone(),
            )
            .map_err(|e| e.to_string())?;
            let started = Started {
                map: None,
                start: Some(ReplayStart::Save(bytes)),
                pending: loaded.pending,
                last_seq: loaded.last_seq,
                dropped: loaded.dropped,
            };
            started.into_match(loaded.app, &content)
        }
        LoadRequest::Rebuild { save, content } => {
            let opts = LoadOptions {
                drop_unknown_units: true,
            };
            let loaded = load_world_with(
                &source.features,
                content.clone(),
                &save,
                source.run.clone(),
                opts,
            )
            .map_err(|e| e.to_string())?;
            // The same match goes on: it keeps its AI host and replay capture.
            let started = Started {
                map: None,
                start: None,
                pending: loaded.pending,
                last_seq: loaded.last_seq,
                dropped: loaded.dropped,
            };
            started.into_match(loaded.app, &content)
        }
    }))
}

/// What a loader knows about a sim it built, besides the sim.
struct Started {
    /// The match's map, when known (a save names none).
    map: Option<String>,
    /// How a replay of a new match sets it up; `None` for a rebuild, which continues the
    /// running match.
    start: Option<ReplayStart>,
    pending: Vec<PlayerCommand>,
    last_seq: BTreeMap<PlayerId, u32>,
    dropped: Vec<(SimId, String)>,
}

impl Started {
    /// Starts the sim of `app` (under `content`) and returns what the client swaps in: the
    /// map view and terrain of the loaded world (so a save shows its own map, though it names
    /// none), and for a new match its AI host (ruling R91) and replay capture.
    ///
    /// # Errors
    /// `unknown AI controller ...` ([`ai_host`]); nothing is started then.
    fn into_match(self, app: App, content: &ContentDb) -> Result<LoadedMatch, String> {
        let world = app.world();
        let terrain = MatchTerrain::of_sim(world, content, self.map.as_deref());
        let map = terrain.map_view(content);
        let (ai, capture) = match self.start {
            Some(start) => (
                Some(ai_host(world.resource::<Players>())?),
                Some(ReplayCapture::of_sim(world, start)),
            ),
            None => (None, None),
        };
        let (link, next) = run_sim(app);
        let hook: LoadHook = Box::new(move |world: &mut World| {
            crate::reload::install_terrain(world, terrain);
            if let Some(capture) = capture {
                world.insert_resource(capture);
            }
        });
        Ok(LoadedMatch {
            link,
            next,
            pending: self.pending,
            last_seq: self.last_seq,
            map: Some(map),
            ai,
            dropped: self.dropped,
            hook: Some(hook),
        })
    }
}

/// The global profiler's snapshot as JSON, for the perf panel and `fl.perf`.
fn profiler_json() -> serde_json::Value {
    serde_json::to_value(global_profiler().snapshot()).unwrap_or(serde_json::Value::Null)
}

/// Adds the remote debug endpoint on `port`.
#[cfg(all(feature = "debug_remote", debug_assertions))]
fn add_remote_debug(app: &mut App, port: u16) {
    app.add_plugins(fl_client::debug::RemoteDebugPlugin { port });
}

/// Without the `debug_remote` feature, and in release builds, there is no remote debug
/// endpoint (spec §15 C).
#[cfg(not(all(feature = "debug_remote", debug_assertions)))]
fn add_remote_debug(_app: &mut App, port: u16) {
    tracing::warn!(
        port,
        "the remote debug endpoint needs the debug_remote feature and a debug build; it is off"
    );
}

/// Checks that every client feature presents a sim feature of the match (the core feature is
/// always in it).
///
/// # Errors
/// `"client feature for '<id>' has no SimFeature"` for the first that does not.
pub fn check_client_features(
    sim: &[Arc<dyn SimFeature>],
    client: &[Arc<dyn ClientFeature>],
) -> Result<(), String> {
    for feature in client {
        let id = feature.sim_feature();
        if id != CoreFeature::ID && !sim.iter().any(|f| f.id() == id) {
            return Err(format!("client feature for '{id}' has no SimFeature"));
        }
    }
    Ok(())
}

/// The client features in the build order of their sim features ([`sort_features`], which
/// puts [`CoreFeature`] first); two for one sim feature keep the order they were added in.
fn sorted_client_features(
    sim: &[Arc<dyn SimFeature>],
    client: &[Arc<dyn ClientFeature>],
) -> Result<Vec<Arc<dyn ClientFeature>>, String> {
    let order: BTreeMap<&'static str, usize> = sort_features(sim)
        .map_err(|e| e.to_string())?
        .iter()
        .enumerate()
        .map(|(i, f)| (f.id(), i))
        .collect();
    let mut sorted = client.to_vec();
    sorted.sort_by_key(|f| order.get(f.sim_feature()).copied().unwrap_or(usize::MAX));
    Ok(sorted)
}

/// Each player's faction colour; a player whose faction is not loaded gets its palette
/// colour.
fn player_colors(content: &ContentDb, players: &Players) -> PlayerColors {
    let factions: Vec<&Faction> = content.kind::<Faction>().iter().collect();
    let palette = PlayerColors::default();
    PlayerColors(
        players
            .list
            .iter()
            .map(|p| {
                let color = factions.get(usize::from(p.faction.0)).map_or_else(
                    || palette.color(p.id),
                    |f| {
                        let (r, g, b) = f.color;
                        Color::linear_rgb(r, g, b)
                    },
                );
                (p.id, color)
            })
            .collect(),
    )
}

/// The debug config of a windowed run, kept for [`observe_layers`] (a `fn` pointer).
#[derive(Resource, Clone)]
struct ObserveConfig(DebugConfig);

/// In debug builds, creates the run directory `<logs_dir>/<run-id>/` as the global run dir; sets
/// the profiler budgets from `cfg`.
fn observe_setup(logs_dir: &Path, cfg: &DebugConfig) {
    if cfg!(debug_assertions) {
        match new_run_dir(logs_dir) {
            Ok(run) => set_global_run_dir(run),
            Err(err) => eprintln!("fl_game: no run directory: {err}"),
        }
    }
    global_profiler().set_budgets(cfg.budgets.clone());
}

/// The targets of the profiled spans and events: bevy_ecs `system` spans, the sim's
/// `sim_tick` spans and phase markers, the client's `frame` span. Repeated in the `LogPlugin`
/// filter so that a plain `RUST_LOG` level, which Bevy adds to that filter last, cannot hide
/// them. At TRACE, so they cap nothing the per-output log filter asks for.
const PROFILED_DIRECTIVES: &str =
    "bevy_ecs::system::function_system=trace,fl_sim::step=trace,fl_sim::phase=trace,fl_game=trace";

/// The `LogPlugin` filter, a global one: Bevy's caps on noisy crates, `debug.ron`'s filter,
/// and the profiled targets at TRACE. It lets through what the log outputs or the profiler
/// want, and only that, so the rest costs nothing; [`observe_layers`] applies `debug.ron`'s
/// filter again to each log output, so the profiled spans are not printed.
fn log_plugin_filter(cfg: &DebugConfig) -> String {
    format!("{DEFAULT_FILTER}{},{PROFILED_DIRECTIVES}", cfg.log_filter)
}

/// [`observe_layers`] boxed for `LogPlugin`.
fn boxed_observe_layers(
    cfg: &DebugConfig,
    run: Option<&RunDir>,
    profiler: &'static Profiler,
) -> BoxedLayer {
    Box::new(observe_layers(cfg, run, profiler))
}

/// `LogPlugin::custom_layer`: stderr and `log.jsonl` (in the global run dir) under
/// `debug.ron`'s filter, and the profiler layer.
fn client_layers(app: &mut App) -> Option<BoxedLayer> {
    let cfg = app.world().get_resource::<ObserveConfig>()?.0.clone();
    Some(boxed_observe_layers(
        &cfg,
        global_run_dir(),
        global_profiler(),
    ))
}

/// `LogPlugin::fmt_layer`: none, since [`client_layers`] has the filtered stderr output (Bevy's
/// own would print everything the permissive filter lets through).
fn no_fmt_layer(app: &mut App) -> Option<BoxedFmtLayer> {
    app.world()
        .contains_resource::<ObserveConfig>()
        .then(|| Box::new(Identity::new()) as BoxedFmtLayer)
}

/// `journal.jsonl` of the run.
#[derive(Resource)]
struct JournalFile(JournalSink);

/// Writes every journal entry received this frame to `journal.jsonl`; on an I/O error, warns
/// once and stops writing.
fn write_journal(
    mut commands: Commands,
    mut received: MessageReader<SimJournalMsg>,
    mut file: ResMut<JournalFile>,
) {
    let entries: Vec<_> = received.read().map(|msg| msg.0.clone()).collect();
    if entries.is_empty() {
        return;
    }
    if let Err(err) = file.0.write(&entries) {
        tracing::warn!(%err, "journal.jsonl cannot be written; the journal file stops here");
        commands.remove_resource::<JournalFile>();
    }
}

/// Closes one `frame` span per client frame: the profiler folds the frame's client system
/// timings when it closes.
fn close_frame_span() {
    drop(tracing::info_span!("frame").entered());
}

/// The `exit_after` timer and the frame times it has seen after the warm-up.
#[derive(Resource)]
struct ExitAfter {
    after: Duration,
    elapsed: Duration,
    units: usize,
    frame_secs: Vec<f32>,
    done: bool,
}

impl ExitAfter {
    fn new(secs: f32, units: usize) -> Self {
        ExitAfter {
            after: Duration::try_from_secs_f32(secs.max(0.0)).unwrap_or(Duration::MAX),
            elapsed: Duration::ZERO,
            units,
            frame_secs: Vec::new(),
            done: false,
        }
    }
}

/// Counts real time; once it reaches the limit, prints the FPS line and exits.
fn exit_after(
    time: Res<Time<Real>>,
    mut state: ResMut<ExitAfter>,
    mut exit: MessageWriter<AppExit>,
) {
    if state.done {
        return;
    }
    let dt = time.delta();
    state.elapsed += dt;
    if state.elapsed > FPS_WARMUP && !dt.is_zero() {
        state.frame_secs.push(dt.as_secs_f32());
    }
    if state.elapsed < state.after {
        return;
    }
    state.done = true;
    let (avg, low) = fps_stats(&state.frame_secs).unwrap_or((0.0, 0.0));
    println!(
        "stress: units={} avg_fps={avg:.1} p1_low_fps={low:.1} frames={}",
        state.units,
        state.frame_secs.len()
    );
    exit.write(AppExit::Success);
}

/// `(average FPS, 1% low FPS)` of these frame times: frames over their total time, and the
/// FPS of the mean of the slowest 1% of frames (at least one). `None` without frames.
fn fps_stats(frame_secs: &[f32]) -> Option<(f32, f32)> {
    let total: f32 = frame_secs.iter().sum();
    if frame_secs.is_empty() || total <= 0.0 {
        return None;
    }
    let mut slowest = frame_secs.to_vec();
    slowest.sort_by(|a, b| b.total_cmp(a));
    let n = slowest.len().div_ceil(100);
    let low_mean = slowest[..n].iter().sum::<f32>() / n as f32;
    let count = frame_secs.len() as f32;
    Some((count / total, 1.0 / low_mean))
}

#[cfg(test)]
mod tests {
    use fl_protocol::Tick;
    use fl_view::JournalEntry;

    use super::*;

    #[test]
    fn fps_stats_average_and_one_percent_low() {
        assert_eq!(fps_stats(&[]), None);
        // 99 frames of 10 ms and one of 50 ms: 100 frames in 1.04 s.
        let mut frames = vec![0.010; 99];
        frames.push(0.050);
        let (avg, low) = fps_stats(&frames).expect("frames");
        assert!((avg - 100.0 / 1.04).abs() < 0.01, "{avg}");
        assert!((low - 20.0).abs() < 0.01, "{low}");
        // 200 frames: the slowest two make up the 1%.
        let mut frames = vec![0.010; 198];
        frames.extend([0.040, 0.060]);
        let (_, low) = fps_stats(&frames).expect("frames");
        assert!((low - 20.0).abs() < 0.01, "{low}");
    }

    #[test]
    fn exit_after_exits_and_skips_the_warm_up() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(
                Duration::from_millis(100),
            ))
            .insert_resource(ExitAfter::new(4.0, 7))
            .add_systems(Last, exit_after);
        let mut updates = 0;
        while app.should_exit().is_none() {
            app.update();
            updates += 1;
            assert!(updates < 100, "exit_after never fired");
        }
        assert_eq!(app.should_exit(), Some(AppExit::Success));
        let state = app.world().resource::<ExitAfter>();
        // The first update has no delta; 40 more reach 4 s, of which those past 3 s count.
        assert_eq!(updates, 41);
        assert_eq!(state.frame_secs.len(), 10);
    }

    #[test]
    fn journal_entries_are_written_to_the_run_dir() {
        let dir = tempfile::tempdir().expect("temp dir");
        let run = fl_observe::RunDir {
            id: "test".to_owned(),
            path: dir.path().to_owned(),
        };
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<SimJournalMsg>()
            .insert_resource(JournalFile(JournalSink::create(&run).expect("sink")))
            .add_systems(
                PostUpdate,
                write_journal.run_if(resource_exists::<JournalFile>),
            );
        let entry = |tick, kind: &str| JournalEntry {
            tick: Tick(tick),
            phase: 0,
            kind: kind.to_owned(),
            sim_id: None,
            player: None,
            emitted: 0,
            data: Default::default(),
        };
        app.world_mut().write_message_batch([
            SimJournalMsg(entry(1, "test.a")),
            SimJournalMsg(entry(1, "test.b")),
        ]);
        app.update();
        app.world_mut()
            .write_message(SimJournalMsg(entry(2, "test.c")));
        app.update();
        app.update();
        let text = std::fs::read_to_string(dir.path().join("journal.jsonl")).expect("journal");
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 3, "{text}");
        for (line, kind) in lines.iter().zip(["test.a", "test.b", "test.c"]) {
            assert!(line.contains(&format!("\"kind\":\"{kind}\"")), "{line}");
        }
    }

    /// The subscriber `LogPlugin` builds around [`client_layers`]: `filter` as the plugin
    /// filter at its default level, then `rust_log` added as `RUST_LOG` is.
    fn log_plugin_subscriber(
        filter: &str,
        rust_log: &str,
        cfg: &DebugConfig,
        profiler: &'static Profiler,
    ) -> impl tracing::Subscriber + Send + Sync {
        use bevy::log::Level;
        use bevy::log::tracing_subscriber::layer::SubscriberExt;
        use bevy::log::tracing_subscriber::{EnvFilter, Registry};

        let global = EnvFilter::builder()
            .parse_lossy(format!("{},{filter}", Level::INFO))
            .add_directive(rust_log.parse().expect("a directive"));
        Registry::default()
            .with(Some(boxed_observe_layers(cfg, None, profiler)))
            .with(global)
    }

    /// Three sim ticks running a sim system, then a client system and a frame, with the
    /// targets they have in the game.
    fn profiled_work() {
        for tick in 0..3_u64 {
            let tick = tracing::info_span!(target: "fl_sim::step", "sim_tick", tick);
            let _tick = tick.enter();
            let system = tracing::info_span!(
                target: "bevy_ecs::system::function_system",
                parent: None,
                "system",
                name = "fl_sim::movement::move_system"
            );
            let _system = system.enter();
        }
        let system = tracing::info_span!(
            target: "bevy_ecs::system::function_system",
            parent: None,
            "system",
            name = "fl_client::view::interpolate_views"
        );
        drop(system.enter());
        close_frame_span();
    }

    fn samples(profiler: &Profiler, system: &str) -> u64 {
        profiler
            .snapshot()
            .systems
            .iter()
            .find(|s| s.name == system)
            .map_or(0, |s| s.stat.samples)
    }

    #[test]
    fn profiler_ignores_a_warn_log_filter() {
        let cfg = DebugConfig {
            log_filter: "warn".to_owned(),
            json_log: false,
            ..DebugConfig::default()
        };
        let profiler: &'static Profiler = Box::leak(Box::new(Profiler::new()));
        let subscriber = log_plugin_subscriber(&log_plugin_filter(&cfg), "warn", &cfg, profiler);
        tracing::subscriber::with_default(subscriber, profiled_work);
        assert_eq!(profiler.snapshot().ticks, 3);
        assert_eq!(samples(profiler, "fl_sim::movement::move_system"), 3);
        assert_eq!(samples(profiler, "fl_client::view::interpolate_views"), 1);

        // Without the profiled directives, the global filter hides every span from the profiler.
        let blind: &'static Profiler = Box::leak(Box::new(Profiler::new()));
        let filter = format!("{DEFAULT_FILTER}{}", cfg.log_filter);
        let subscriber = log_plugin_subscriber(&filter, "warn", &cfg, blind);
        tracing::subscriber::with_default(subscriber, profiled_work);
        assert_eq!(blind.snapshot().ticks, 0);
        assert_eq!(samples(blind, "fl_sim::movement::move_system"), 0);
    }

    fn write_debug_ron(dir: &Path, body: &str) -> PathBuf {
        let path = dir.join("debug.ron");
        std::fs::write(&path, body).expect("write debug.ron");
        path
    }

    #[test]
    fn windowed_startup_reads_debug_ron_before_the_sim_starts() {
        let dir = tempfile::tempdir().expect("temp dir");
        let on = write_debug_ron(dir.path(), "(profiler: true)");
        let (cfg, run) = GameBuilder::new()
            .debug_ron(&on)
            .startup_config(Shell::Windowed);
        assert!(cfg.expect("windowed shells read debug.ron").profiler);
        assert!(run.phase_markers, "the profiler needs the phase markers");

        let off = write_debug_ron(dir.path(), "(profiler: false)");
        let (cfg, run) = GameBuilder::new()
            .debug_ron(&off)
            .startup_config(Shell::Windowed);
        assert!(!cfg.expect("windowed shells read debug.ron").profiler);
        assert!(!run.phase_markers);

        // The headless shell has no debug.ron and no markers, whatever the file says.
        let (cfg, run) = GameBuilder::new()
            .debug_ron(&on)
            .startup_config(Shell::Headless);
        assert!(cfg.is_none() && !run.phase_markers);
    }

    #[test]
    fn content_error_leaves_no_run_directory() {
        let dir = tempfile::tempdir().expect("temp dir");
        let ron = write_debug_ron(dir.path(), "(profiler: true)");
        let logs = dir.path().join("logs");
        let bad_mod =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../fl_cli/tests/fixtures/bad_mod");
        let Err(error) = GameBuilder::new()
            .content_mod(bad_mod)
            .debug_ron(ron)
            .logs_dir(&logs)
            .build(Shell::Windowed)
        else {
            panic!("a broken mod must not build");
        };
        assert!(error.contains("bogus_component.unit.ron"), "{error}");
        let entries = std::fs::read_dir(&logs).map_or(0, Iterator::count);
        assert_eq!(entries, 0, "no run directory for a run that never started");
    }

    /// A sim feature with dependencies and nothing to build.
    struct Sim(&'static str, &'static [&'static str]);

    impl SimFeature for Sim {
        fn id(&self) -> &'static str {
            self.0
        }
        fn version(&self) -> u32 {
            1
        }
        fn deps(&self) -> &'static [&'static str] {
            self.1
        }
        fn build(&self, _sim: &mut fl_sim::SimBuilder) {}
    }

    /// The order client features were built in, by tag.
    #[derive(Resource, Default)]
    struct BuildOrder(Vec<&'static str>);

    /// A client feature presenting sim feature `.0` that records its tag `.1` when built.
    struct Client(&'static str, &'static str);

    impl ClientFeature for Client {
        fn sim_feature(&self) -> &'static str {
            self.0
        }
        fn build(&self, app: &mut App) {
            app.world_mut().resource_mut::<BuildOrder>().0.push(self.1);
        }
    }

    #[test]
    fn client_features_follow_the_sim_feature_order() {
        let sim: Vec<Arc<dyn SimFeature>> =
            vec![Arc::new(Sim("a", &["b"])), Arc::new(Sim("b", &[]))];
        let client: Vec<Arc<dyn ClientFeature>> = vec![
            Arc::new(Client("a", "a")),
            Arc::new(Client("b", "b1")),
            Arc::new(Client("core", "core")),
            Arc::new(Client("b", "b2")),
        ];
        let mut app = App::new();
        app.init_resource::<BuildOrder>();
        for feature in sorted_client_features(&sim, &client).expect("orderable") {
            feature.build(&mut app);
        }
        assert_eq!(
            app.world().resource::<BuildOrder>().0,
            ["core", "b1", "b2", "a"]
        );
    }
}
