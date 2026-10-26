//! The headless session host (`npcctl serve`): one scenario running in real
//! time behind the JSON-RPC server ([`crate::http`]).
//!
//! The loop ([`Session::run`]) never blocks on a client:
//! - every iteration drains the request channel ([`Session::pump`]), answers
//!   instant methods at once and parks `agent/events` long-polls in a list;
//!   parked polls are answered as soon as a matching event is buffered
//!   (events are harvested from the journal right after every frame), or
//!   when their deadline passes; `agent/act_and_wait` acts at once and parks
//!   its wait the same way (answered when the unit is idle, a named kind
//!   fires, or at the deadline);
//! - a frame (`App::update`, the headless driver running
//!   `SimControl.multiplier` ticks, 0 while paused) runs every
//!   `1 / tick_hz` seconds of wall time; between frames the loop sleeps at
//!   most 1 ms, so a request waits at most ~1 ms plus the frame in progress.
//!
//! So an action-done event reaches a waiting poll in the same loop
//! iteration as the tick that produced it, i.e. within one frame.
//!
//! `npc/step {ticks}` never blocks the loop either: more than
//! [`STEP_SLICE`] ticks run in slices of at most [`STEP_SLICE`] ticks
//! (about [`STEP_BUDGET`] of wall time per loop iteration), with requests,
//! long-polls and event harvesting serviced between slices; the call is
//! answered when the last slice is done. Frames do not run while a step is
//! in progress (the step owns the clock, as it did when it ran at once);
//! further steps queue behind it. `world/reset`, `world/load_scenario` and
//! `session/stop` answer a step in progress with an error.
//!
//! Session methods (besides [`crate::api::METHODS`] and
//! [`crate::api::NPC_METHODS`]): `world/load_scenario {scenario, seed?}` (the
//! scenario document inline; the server never reads a client-named file),
//! `world/reset {seed?}` (rebuild the app from the scenario: reset is a
//! replay of seed + scenario, here without commands), `session/info` and
//! `session/stop`.
//!
//! **Stopping.** `session/stop` (or [`SessionOptions::stop`]) first runs
//! [`SessionOptions::on_stop`] (a final save); when that refuses, the call
//! gets the error and nothing else changes. Otherwise the stop is answered
//! and from then on the world is frozen: every call still queued or arriving
//! before the server closes, every parked long-poll and every step in
//! progress is answered with an error starting "the session is stopping"
//! (nothing more runs on the world, so the final save is the last state),
//! `--record` is written and [`Session::run`] returns.
//!
//! [`Session::start_app`] hosts a world its host built (the detached
//! explorer, `npcctl serve --explorer-home`) instead of a scenario: no
//! `--record`, `world/load_scenario` refused, `world/reset` only through
//! [`SessionOptions::rebuild`]. Hooks: [`SessionOptions::on_frame`] runs
//! between frames (an autosave), [`SessionOptions::on_stop`] when a stop is
//! asked for (a final save; an error refuses `session/stop` and the world
//! keeps running), and [`SessionOptions::stop`] ends the loop from outside
//! (Ctrl+C). The hooks run in the host loop, between ticks; they never
//! change what a tick does.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use bevy_app::{App, PluginsState};
use bevy_ecs::world::World;
use npc_kernel::replay::Recorder;
use npc_tools::HeadlessDriverPlugin;
use serde_json::{Value, json};

use crate::api::{self, ActWait, EventsParams, RpcError, RpcResult, codes};
use crate::events;
use crate::http::{HttpServer, Incoming, ServerOptions};
use crate::scenario::{self, ScenarioDoc, ScenarioLoaders, SessionRecord};
use crate::stats::wall_now;

/// Checkpoint interval of the session recording, in ticks.
pub const RECORD_EVERY: u64 = 100;
/// `npc/step` runs at most this many ticks between two looks at the
/// request channel.
pub const STEP_SLICE: u64 = 50;
/// Wall time a step may take per loop iteration before the loop services
/// requests again.
pub const STEP_BUDGET: Duration = Duration::from_millis(5);
/// Most ticks one `npc/step` may ask for (as on BRP).
pub const MAX_STEP: u64 = 1_000_000;

/// A host hook on the session's world, run between ticks.
pub type WorldHook = Box<dyn FnMut(&mut World)>;
/// [`SessionOptions::on_stop`]: `Ok(extra)` lets the stop go ahead (an
/// object's fields join the `session/stop` answer), `Err` refuses it.
pub type StopHook = Box<dyn FnMut(&mut World) -> Result<Value, String>>;
/// [`SessionOptions::rebuild`]: the world for `world/reset {seed}`.
pub type RebuildHook = Box<dyn FnMut(u64) -> Result<App, String>>;

/// Session settings.
pub struct SessionOptions {
    /// `0`: any free port.
    pub port: u16,
    /// Frames (ticks at speed 1) per second.
    pub tick_hz: u32,
    /// Write the session record here on `session/stop` (scenario sessions
    /// only).
    pub record: Option<PathBuf>,
    /// Overrides the scenario's seed; for [`Session::start_app`], the
    /// world's seed (reported, and `world/reset`'s default).
    pub seed: Option<u64>,
    /// Require `Authorization: Bearer <token>` (`npcctl serve --token`).
    pub token: Option<String>,
    /// Runs after every frame and every `npc/step` slice.
    pub on_frame: Option<WorldHook>,
    /// Runs when a stop is asked for (`session/stop` or [`Self::stop`]).
    /// An `Err` answers `session/stop` with it and the session keeps
    /// running; on the stop flag the loop ends anyway and [`Session::run`]
    /// returns the error.
    pub on_stop: Option<StopHook>,
    /// [`Session::start_app`]: how `world/reset {seed?}` rebuilds the world
    /// (`None`: reset is refused). The new app is prepared like the first.
    pub rebuild: Option<RebuildHook>,
    /// Set from outside (a Ctrl+C handler): the loop stops as on
    /// `session/stop`.
    pub stop: Option<Arc<AtomicBool>>,
}

impl Default for SessionOptions {
    fn default() -> Self {
        Self {
            port: 15702,
            tick_hz: npc_kernel::clock::DEFAULT_TICK_HZ,
            record: None,
            seed: None,
            token: None,
            on_frame: None,
            on_stop: None,
            rebuild: None,
            stop: None,
        }
    }
}

impl std::fmt::Debug for SessionOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SessionOptions")
            .field("port", &self.port)
            .field("tick_hz", &self.tick_hz)
            .field("record", &self.record)
            .field("seed", &self.seed)
            .field("token", &self.token.as_ref().map(|_| "<set>"))
            .field("on_frame", &self.on_frame.is_some())
            .field("on_stop", &self.on_stop.is_some())
            .field("rebuild", &self.rebuild.is_some())
            .field("stop", &self.stop)
            .finish()
    }
}

/// What the session's world is built from.
enum Source {
    /// A scenario document ([`Session::start`]): reset and load rebuild it.
    Scenario {
        loaders: ScenarioLoaders,
        doc: ScenarioDoc,
    },
    /// An app its host built ([`Session::start_app`]).
    App,
}

struct Poll {
    call: Incoming,
    params: EventsParams,
    deadline: Instant,
}

/// A parked `agent/act_and_wait` (its act is done).
struct ActPoll {
    call: Incoming,
    wait: ActWait,
    deadline: Instant,
}

/// An `npc/step` in progress.
struct PendingStep {
    call: Incoming,
    ticks: u64,
    left: u64,
}

/// `npc/step` params as `call_npc` reads them: `Some(ticks)` when valid.
fn step_ticks(params: Option<&Value>) -> Option<u64> {
    match params {
        None | Some(Value::Null) => Some(1),
        Some(Value::Object(m)) => match (m.len(), m.get("ticks")) {
            (0, _) => Some(1),
            (1, Some(t)) => t.as_u64(),
            _ => None,
        },
        _ => None,
    }
}

/// A running session.
pub struct Session {
    source: Source,
    seed: u64,
    app: App,
    /// Scenario sessions only (a recording replays scenario + seed).
    recorder: Option<Recorder>,
    server: HttpServer,
    polls: Vec<Poll>,
    act_polls: Vec<ActPoll>,
    steps: VecDeque<PendingStep>,
    options: SessionOptions,
    stopping: bool,
}

fn build(
    loaders: &ScenarioLoaders,
    doc: &ScenarioDoc,
    seed: u64,
) -> Result<(App, Recorder), String> {
    let mut loaded = scenario::load(loaders, doc, Some(seed))?;
    let app = &mut loaded.app;
    prepare(app)?;
    let recorder = Recorder::live(app.world_mut(), RECORD_EVERY);
    Ok((loaded.app, recorder))
}

/// The answer to every call after a stop was accepted.
pub const STOPPING: &str = "the session is stopping";

/// Makes `app` ready for the loop: adds the headless driver unless the host
/// added a driver (`HeadlessDriverPlugin` or `NpcDriverPlugin`), then
/// finishes and cleans up the plugins unless the host did.
///
/// # Errors
/// An app whose plugins are finished already but that has no driver: a
/// plugin can no longer be added, and without one it would never tick.
fn prepare(app: &mut App) -> Result<(), String> {
    let driven = app.is_plugin_added::<HeadlessDriverPlugin>()
        || app.is_plugin_added::<npc_kernel::NpcDriverPlugin>();
    match app.plugins_state() {
        PluginsState::Adding | PluginsState::Ready => {
            if !driven {
                app.add_plugins(HeadlessDriverPlugin);
            }
            app.finish();
            app.cleanup();
        }
        state @ (PluginsState::Finished | PluginsState::Cleaned) => {
            if !driven {
                return Err(format!(
                    "the app's plugins are already {state:?} and it has no driver: add \
                     HeadlessDriverPlugin before finishing it, or leave finishing to the session"
                ));
            }
            if state == PluginsState::Finished {
                app.cleanup();
            }
        }
    }
    Ok(())
}

/// Answers a call that came after the stop.
fn refuse_stopping(call: Incoming) {
    call.reply(Err(RpcError::new(
        codes::INTERNAL,
        format!("{STOPPING}: this call was not run"),
    )));
}

/// Listens on `options.port`.
fn bind(options: &SessionOptions) -> Result<HttpServer, String> {
    HttpServer::bind_with(
        options.port,
        ServerOptions {
            token: options.token.clone(),
        },
    )
    .map_err(|e| format!("cannot listen on 127.0.0.1:{}: {e}", options.port))
}

impl Session {
    /// Loads `doc` and binds the server.
    pub fn start(
        loaders: ScenarioLoaders,
        doc: ScenarioDoc,
        options: SessionOptions,
    ) -> Result<Self, String> {
        let seed = options.seed.or(doc.seed).unwrap_or(1);
        let (app, recorder) = build(&loaders, &doc, seed)?;
        let server = bind(&options)?;
        Ok(Self::new(
            Source::Scenario { loaders, doc },
            seed,
            app,
            Some(recorder),
            server,
            options,
        ))
    }

    /// Hosts `app`, a world its host built (and may already have loaded
    /// from a save): adds the headless driver unless present, finishes the
    /// plugins unless finished, and binds `options.port`. The sim's pause
    /// state is the app's.
    ///
    /// # Errors
    /// `record` is set (a recording needs scenario + seed from tick 0), or
    /// the port cannot be bound.
    pub fn start_app(app: App, options: SessionOptions) -> Result<Self, String> {
        let server = bind(&options)?;
        Self::start_app_on(app, server, options)
    }

    /// [`Self::start_app`] on a server the host bound already (so a taken
    /// port is found before the host writes anything); `options.port` and
    /// `options.token` are not used.
    ///
    /// # Errors
    /// `record` is set, or the app is finished without a driver.
    pub fn start_app_on(
        mut app: App,
        server: HttpServer,
        options: SessionOptions,
    ) -> Result<Self, String> {
        if options.record.is_some() {
            return Err(
                "a session hosting a built world cannot record (a recording replays \
                 scenario + seed from tick 0)"
                    .into(),
            );
        }
        prepare(&mut app)?;
        let seed = options.seed.unwrap_or(0);
        Ok(Self::new(Source::App, seed, app, None, server, options))
    }

    fn new(
        source: Source,
        seed: u64,
        app: App,
        recorder: Option<Recorder>,
        server: HttpServer,
        options: SessionOptions,
    ) -> Self {
        Self {
            source,
            seed,
            app,
            recorder,
            server,
            polls: Vec::new(),
            act_polls: Vec::new(),
            steps: VecDeque::new(),
            options,
            stopping: false,
        }
    }

    pub fn url(&self) -> String {
        self.server.url()
    }

    pub fn port(&self) -> u16 {
        self.server.port()
    }

    pub fn app_mut(&mut self) -> &mut App {
        &mut self.app
    }

    /// Runs until `session/stop`; returns the record path written, if any.
    pub fn run(mut self) -> Result<Option<PathBuf>, String> {
        let period = Duration::from_nanos(1_000_000_000 / u64::from(self.options.tick_hz.max(1)));
        let mut next = wall_now();
        let mut written = None;
        let mut stop_error = None;
        while !self.stopping {
            if self.flagged() {
                // Ctrl+C: the stop goes ahead even when the hook fails.
                stop_error = self.run_on_stop().err();
                break;
            }
            self.pump();
            if self.stopping {
                break;
            }
            if !self.steps.is_empty() {
                self.step_slice();
                // Frames resume on the wall clock once the step is done.
                next = wall_now() + period;
                continue;
            }
            let now = wall_now();
            if now >= next {
                self.frame();
                next += period;
                if now > next + period * 10 {
                    // Far behind (a long step): drop the backlog.
                    next = now + period;
                }
            } else {
                std::thread::sleep((next - now).min(Duration::from_millis(1)));
            }
        }
        if let Some(path) = self.options.record.clone() {
            self.write_record(&path)?;
            written = Some(path);
        }
        // Answer what is still parked or queued; nothing runs any more.
        for p in std::mem::take(&mut self.polls) {
            p.call.reply(Err(RpcError::new(codes::INTERNAL, STOPPING)));
        }
        for p in std::mem::take(&mut self.act_polls) {
            p.call.reply(Err(RpcError::new(
                codes::INTERNAL,
                format!("{STOPPING} (the act was submitted before the stop)"),
            )));
        }
        self.abort_steps(STOPPING);
        while let Some(call) = self.server.try_recv() {
            refuse_stopping(call);
        }
        match stop_error {
            Some(e) => Err(e),
            None => Ok(written),
        }
    }

    /// The stop flag ([`SessionOptions::stop`]) is set.
    fn flagged(&self) -> bool {
        self.options
            .stop
            .as_ref()
            .is_some_and(|f| f.load(Ordering::SeqCst))
    }

    /// [`SessionOptions::on_stop`] on the world; `Ok(Null)` without one.
    fn run_on_stop(&mut self) -> Result<Value, String> {
        match self.options.on_stop.as_mut() {
            Some(hook) => hook(self.app.world_mut()),
            None => Ok(Value::Null),
        }
    }

    /// [`SessionOptions::on_frame`] on the world.
    fn run_on_frame(&mut self) {
        if let Some(hook) = self.options.on_frame.as_mut() {
            hook(self.app.world_mut());
        }
    }

    /// Runs the front step for up to [`STEP_BUDGET`], in slices of at most
    /// [`STEP_SLICE`] ticks, and answers it when it is done.
    fn step_slice(&mut self) {
        let start = wall_now();
        while let Some(step) = self.steps.front_mut() {
            let n = step.left.min(STEP_SLICE);
            let world = self.app.world_mut();
            npc_kernel::run_ticks(world, n);
            if let Some(recorder) = self.recorder.as_mut() {
                recorder.after_tick(world);
            }
            events::harvest(world);
            step.left -= n;
            if step.left == 0 {
                let Some(done) = self.steps.pop_front() else {
                    break;
                };
                let world = self.app.world_mut();
                let tick = world.resource::<npc_kernel::SimClock>().tick;
                let hash = npc_kernel::hash::state_hash(world);
                done.call.reply(Ok(json!({
                    "tick": tick, "ticks": done.ticks, "hash": hash,
                    "hash_hex": format!("{hash:016x}"),
                })));
            }
            if wall_now().duration_since(start) >= STEP_BUDGET {
                break;
            }
        }
        self.run_on_frame();
    }

    /// Answers every step in progress with an error.
    fn abort_steps(&mut self, why: &str) {
        for s in std::mem::take(&mut self.steps) {
            let done = s.ticks - s.left;
            s.call.reply(Err(RpcError::new(
                codes::INTERNAL,
                format!("{why}: npc/step stopped after {done} of {} ticks", s.ticks),
            )));
        }
    }

    /// `true` while an `npc/step` is still running (tests).
    pub fn stepping(&self) -> bool {
        !self.steps.is_empty()
    }

    /// One frame: the headless driver's ticks, a checkpoint when due,
    /// events, then [`SessionOptions::on_frame`].
    pub fn frame(&mut self) {
        self.app.update();
        let world = self.app.world_mut();
        if let Some(recorder) = self.recorder.as_mut() {
            recorder.after_tick(world);
        }
        events::harvest(world);
        self.run_on_frame();
    }

    /// The session record of the run so far.
    ///
    /// # Errors
    /// A session hosting a built world has none ([`Self::start_app`]).
    pub fn record(&mut self) -> Result<SessionRecord, String> {
        let (Source::Scenario { doc, .. }, Some(recorder)) = (&self.source, &self.recorder) else {
            return Err("a session hosting a built world has no record".into());
        };
        let doc = doc.clone();
        let replay = recorder.clone().finish(self.app.world_mut())?;
        Ok(SessionRecord {
            version: scenario::SESSION_VERSION,
            scenario: doc,
            seed: self.seed,
            replay,
        })
    }

    fn write_record(&mut self, path: &std::path::Path) -> Result<(), String> {
        let record = self.record()?;
        let text = serde_json::to_string_pretty(&record).map_err(|e| e.to_string())?;
        if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
            std::fs::create_dir_all(dir)
                .map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
        }
        std::fs::write(path, text).map_err(|e| format!("cannot write {}: {e}", path.display()))
    }

    /// Answers every waiting call and due poll (never blocks).
    pub fn pump(&mut self) {
        while let Some(call) = self.server.try_recv() {
            if self.stopping {
                // The final save is taken: the world must not change.
                refuse_stopping(call);
            } else {
                self.handle(call);
            }
        }
        if self.stopping {
            // `run` answers the parked calls.
            return;
        }
        let now = wall_now();
        let world = self.app.world_mut();
        let mut keep = Vec::new();
        for p in std::mem::take(&mut self.polls) {
            let ready = api::events_ready(world, &p.params);
            if ready || now >= p.deadline {
                let result = api::events_result(world, &p.params, !ready);
                p.call.reply(result);
            } else {
                keep.push(p);
            }
        }
        self.polls = keep;
        let mut keep = Vec::new();
        for p in std::mem::take(&mut self.act_polls) {
            let ready = api::act_wait_ready(world, &p.wait);
            if ready || now >= p.deadline {
                let result = api::act_wait_result(world, &p.wait, !ready);
                p.call.reply(result);
            } else {
                keep.push(p);
            }
        }
        self.act_polls = keep;
    }

    /// A scenario session's world rebuilt from `doc` and `seed`.
    fn rebuild(&mut self, doc: ScenarioDoc, seed: u64) -> RpcResult {
        let Source::Scenario { loaders, .. } = &mut self.source else {
            return Err(RpcError::invalid(
                "this session hosts a world its host built: no scenario to rebuild",
            ));
        };
        let (app, recorder) = build(loaders, &doc, seed).map_err(RpcError::invalid)?;
        let loaders = std::mem::take(loaders);
        self.source = Source::Scenario { loaders, doc };
        self.swap_world(app, Some(recorder), seed);
        Ok(self.info())
    }

    /// `world/reset {seed?}` of a [`Self::start_app`] session: the host's
    /// [`SessionOptions::rebuild`].
    fn rebuild_app(&mut self, seed: u64) -> RpcResult {
        let Some(hook) = self.options.rebuild.as_mut() else {
            return Err(RpcError::invalid(
                "this session hosts a world its host built and cannot reset it",
            ));
        };
        let mut app = hook(seed).map_err(RpcError::invalid)?;
        prepare(&mut app).map_err(RpcError::invalid)?;
        self.swap_world(app, None, seed);
        Ok(self.info())
    }

    /// Replaces the world; answers what waited on the old one.
    fn swap_world(&mut self, app: App, recorder: Option<Recorder>, seed: u64) {
        self.abort_steps("the world was reset");
        for p in std::mem::take(&mut self.polls) {
            p.call.reply(Err(RpcError::new(
                codes::INTERNAL,
                "the world was reset; poll again",
            )));
        }
        for p in std::mem::take(&mut self.act_polls) {
            p.call.reply(Err(RpcError::new(
                codes::INTERNAL,
                "the world was reset while waiting (the act had been submitted); observe again",
            )));
        }
        self.app = app;
        self.recorder = recorder;
        self.seed = seed;
    }

    fn info(&mut self) -> Value {
        let world = self.app.world_mut();
        let tick = world.resource::<npc_kernel::SimClock>().tick;
        let units = npc_kernel::inspect::sim_entities(world).len();
        let mut methods: Vec<&str> = api::METHODS.to_vec();
        methods.extend(api::NPC_METHODS);
        methods.extend(SESSION_METHODS);
        let (scenario, content) = match &self.source {
            Source::Scenario { doc, .. } => (
                doc.name.clone().unwrap_or_else(|| doc.content.clone()),
                Value::String(doc.content.clone()),
            ),
            Source::App => (
                world
                    .get_resource::<npc_kernel::replay::ScenarioInfo>()
                    .map_or_else(|| "app".to_owned(), |i| i.name.clone()),
                Value::Null,
            ),
        };
        json!({
            "scenario": scenario,
            "content": content,
            "seed": self.seed,
            "tick": tick,
            "units": units,
            "tick_hz": self.options.tick_hz,
            "url": self.server.url(),
            "record": self.options.record.as_ref().map(|p| p.display().to_string()),
            "methods": methods,
        })
    }

    fn handle(&mut self, call: Incoming) {
        let method = call.method.clone();
        let params = call.params.clone();
        let result = match method.as_str() {
            "session/info" => Ok(self.info()),
            "session/stop" => match self.run_on_stop() {
                Err(e) => Err(RpcError::new(
                    codes::INTERNAL,
                    format!("{e}; the session keeps running"),
                )),
                Ok(extra) => {
                    self.stopping = true;
                    let mut answer = json!({
                        "stopped": true,
                        "record": self.options.record.as_ref().map(|p| p.display().to_string()),
                    });
                    if let (Some(a), Value::Object(extra)) = (answer.as_object_mut(), extra) {
                        // The hook adds keys; it never replaces `stopped` or `record`.
                        for (k, v) in extra {
                            a.entry(k).or_insert(v);
                        }
                    }
                    Ok(answer)
                }
            },
            "world/reset" => {
                let seed = params
                    .as_ref()
                    .and_then(|p| p.get("seed"))
                    .and_then(Value::as_u64)
                    .unwrap_or(self.seed);
                match &self.source {
                    Source::Scenario { doc, .. } => {
                        let doc = doc.clone();
                        self.rebuild(doc, seed)
                    }
                    Source::App => self.rebuild_app(seed),
                }
            }
            "world/load_scenario" => self.load_scenario(params),
            "npc/methods" => Ok(self.info()["methods"].clone()),
            "npc/step" => match step_ticks(params.as_ref()) {
                // Long steps run in slices across loop iterations.
                // A step behind a running one waits its turn.
                Some(ticks)
                    if ticks <= MAX_STEP && (ticks > STEP_SLICE || !self.steps.is_empty()) =>
                {
                    self.steps.push_back(PendingStep {
                        call,
                        ticks,
                        left: ticks,
                    });
                    return;
                }
                // Short, or invalid params (call_npc reports them).
                _ => api::call_npc(self.app.world_mut(), "npc/step", params),
            },
            "agent/events" => match api::parse_events(params) {
                Err(e) => Err(e),
                Ok(p) => {
                    let world = self.app.world_mut();
                    let timeout = api::events_timeout_ms(world, &p);
                    if timeout == 0 || api::events_ready(world, &p) {
                        api::events_result(world, &p, false)
                    } else {
                        self.polls.push(Poll {
                            call,
                            params: p,
                            deadline: wall_now() + Duration::from_millis(timeout),
                        });
                        return;
                    }
                }
            },
            "agent/act_and_wait" => {
                let world = self.app.world_mut();
                match api::act_and_wait_start(world, params) {
                    Err(e) => Err(e),
                    Ok(w) if w.answers_at_once() || api::act_wait_ready(world, &w) => {
                        api::act_wait_result(world, &w, false)
                    }
                    Ok(w) => {
                        let deadline = wall_now() + Duration::from_millis(w.timeout_ms);
                        self.act_polls.push(ActPoll {
                            call,
                            wait: w,
                            deadline,
                        });
                        return;
                    }
                }
            }
            m if api::NPC_METHODS.contains(&m) => api::call_npc(self.app.world_mut(), m, params),
            m => api::call(self.app.world_mut(), m, params),
        };
        call.reply(result);
    }

    /// `world/load_scenario {scenario, seed?}`: the document inline (`npcctl
    /// world load-scenario FILE` reads the file on the client side). There
    /// is deliberately no `path`: a server that opened client-named files
    /// would be a file-existence oracle for any page that reaches the port.
    fn load_scenario(&mut self, params: Option<Value>) -> RpcResult {
        if matches!(self.source, Source::App) {
            return Err(RpcError::invalid(
                "this session hosts a world its host built (an explorer world builds its \
                 own): world/load_scenario is not supported here; world/reset rebuilds it \
                 when the host allows",
            ));
        }
        let p = params.unwrap_or(Value::Null);
        if let Some(obj) = p.as_object()
            && let Some(k) = obj
                .keys()
                .find(|k| !matches!(k.as_str(), "scenario" | "seed"))
        {
            return Err(RpcError::invalid(format!(
                "unknown param `{k}`: give {{\"scenario\": {{..}}, \"seed\"?}} (the scenario \
                 document inline; `path` is not supported, the server reads no client files)"
            )));
        }
        let seed = p.get("seed").and_then(Value::as_u64);
        let doc = if let Some(s) = p.get("scenario") {
            ScenarioDoc::from_value(s.clone()).map_err(RpcError::invalid)?
        } else {
            return Err(RpcError::invalid(
                "give {\"scenario\": {..}, \"seed\"?} (the scenario document inline)",
            ));
        };
        let seed = seed.or(doc.seed).unwrap_or(1);
        self.rebuild(doc, seed)
    }
}

/// Methods only the session host serves.
pub const SESSION_METHODS: [&str; 5] = [
    "world/load_scenario",
    "world/reset",
    "session/info",
    "session/stop",
    "npc/methods",
];
