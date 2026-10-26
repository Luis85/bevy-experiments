//! Bevy Remote Protocol (JSON-RPC 2.0 over HTTP) methods for the NPC sim.
//!
//! [`NpcRemotePlugin`] installs Bevy's `RemotePlugin` (all built-in `world.*`,
//! `registry.*`, `schedule.*` and `rpc.discover` methods) plus the `npc/*`
//! methods in [`methods`], and `RemoteHttpPlugin` on `127.0.0.1:<port>` (the
//! render-app server on `port + 1`, see [`RenderPort`]). Both ports are
//! probed first; a taken port is logged with its number ([`RemoteStatus`]).
//! POST JSON-RPC to `http://127.0.0.1:15702/`:
//!
//! ```text
//! curl -s -X POST http://127.0.0.1:15702/ -d '{"jsonrpc":"2.0","id":1,"method":"npc/state"}'
//! ```
//!
//! | method | params | result |
//! |---|---|---|
//! | `npc/pause`, `npc/resume` | – | [`ControlState`] |
//! | `npc/step` | `{ticks = 1}` | [`StepResult`] |
//! | `npc/speed` | `{multiplier}` | [`ControlState`] |
//! | `npc/state` | – | the installed inspector's world JSON (village: `WorldSnapshot`) |
//! | `npc/unit` | `{id}` | the installed inspector's unit JSON (village: `UnitSnapshot`) |
//! | `npc/command` | `{cmd: SimCommand, at_tick?}` | [`CommandAccepted`] |
//! | `npc/journal` | `{filter: JournalFilter}` or the filter fields | `[JournalEntry]` |
//! | `npc/perf` | `{top?}` | `npc_perf::PerfReport` |
//! | `npc/perf/reset` | – | `{"reset": true}` |
//! | `npc/record/start` | `{every = 100}` | [`RecordStatus`] |
//! | `npc/record/stop` | `{path?, overwrite?}` (relative, under [`OutputRoot`](crate::out_path::OutputRoot)) | `npc_kernel::replay::Replay` |
//! | `npc/methods` | – | the `npc/*` names |
//!
//! Errors are JSON-RPC errors (`BrpError`): `-32602` bad params or a
//! rejected command, `-23401` unknown unit, `-23502` missing sim resource
//! or profiler, `-23501` journal off, `-32603` internal. Command rejections
//! carry the `CommandError` JSON in `data` (`{"error":"UnknownUnit","unit":999}`).
//!
//! Pause and speed act on the kernel's `SimControl`, whichever driver runs
//! the sim (the game's real-time `NpcDriverPlugin`, or headless
//! [`HeadlessDriverPlugin`](crate::HeadlessDriverPlugin)). Handlers run with
//! exclusive world access in
//! `RemoteLast` (after `Last`), so `npc/step` runs its ticks synchronously.
//! Games add their own methods with [`RemoteAppExt::add_remote_method`].
//!
//! **Security** (`docs/debugging.md` §3.2): `bevy_remote` 0.20 offers no hook
//! to inspect a request before it is dispatched (`RemoteHttpPlugin::with_headers`
//! only adds *response* headers), so this server cannot refuse cross-origin
//! (`Origin`), wrong-`Host` (DNS rebinding) or non-JSON requests the way
//! `npc_agent_io`'s session server does. What it can do: listen on
//! `127.0.0.1` only, and confine every client-supplied file path under the
//! [`OutputRoot`](crate::out_path::OutputRoot) (`npc/record/stop {path}`,
//! the apps' `npc/screenshot {path}`). The apps keep BRP off in release
//! builds unless `--remote` is given; outside development, run with
//! `--no-remote`.

pub mod methods;

use bevy::prelude::*;
use bevy::remote::http::{HostPort, RemoteHttpPlugin};
use bevy::remote::{RemoteMethodSystemId, RemoteMethods, RemotePlugin};
use bevy::render::RenderApp;
use serde::Serialize;
use serde_json::Value;

/// BRP types for crates registering their own methods (e.g. `npc_agent_io`)
/// without depending on the `bevy` umbrella themselves.
pub use bevy::remote::{BrpError, BrpResult, error_codes};
pub use methods::{
    ActiveRecording, CommandAccepted, ControlState, METHODS, RecordOnExit, RecordStatus, Recording,
    StepResult, command_error, start_recording, step_ticks, stop_recording, write_replay,
};

/// Bevy's default BRP port.
pub const DEFAULT_PORT: u16 = 15702;

/// Where the render sub-app's BRP server listens. `RemoteHttpPlugin` starts
/// a second server for the render world (`with_method_render` methods);
/// Bevy 0.20 has no setter for its port (always 15703), so
/// [`NpcRemotePlugin`] overrides it (or keeps it from starting).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RenderPort {
    /// `port + 1` (Bevy's default pair 15702/15703); off when `port` is 65535.
    #[default]
    Next,
    /// This port.
    Port(u16),
    /// No render-app server.
    Off,
}

impl RenderPort {
    /// The render server's port for main port `port`, or `None` when off.
    pub fn resolve(self, port: u16) -> Option<u16> {
        match self {
            RenderPort::Next => port.checked_add(1),
            RenderPort::Port(p) => Some(p),
            RenderPort::Off => None,
        }
    }
}

/// What [`NpcRemotePlugin`] did with its ports (inserted when it is built).
#[derive(Resource, Clone, Debug, PartialEq, Eq, Serialize)]
pub struct RemoteStatus {
    /// The main BRP port.
    pub port: u16,
    /// Whether its HTTP server was set up (false when the port was taken).
    pub listening: bool,
    /// The render-app server's port, when that server is set up.
    pub render_port: Option<u16>,
    /// Why a server was not set up (each message names the port).
    pub errors: Vec<String>,
}

/// Checks that `127.0.0.1:port` can be bound right now (the listener is
/// closed again). Bevy binds in a detached task and drops the error, so
/// [`NpcRemotePlugin`] probes first to report a conflict.
pub fn probe_port(port: u16) -> std::io::Result<()> {
    std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port)).map(drop)
}

/// The remote interface: Bevy's `RemotePlugin` with the `npc/*` methods and
/// an HTTP transport on `127.0.0.1:port` (the render-app server per
/// `render_port`). Requires `NpcPlugins` for the sim methods to do anything
/// (without it they return "sim not ready" errors).
///
/// Both ports are probed when the plugin is built: a port that is already
/// taken is logged as an error naming it (target `npc_tools`) and its server
/// is not started; [`RemoteStatus`] records the outcome. The `npc/*`
/// methods stay registered either way (in-process callers still work).
pub struct NpcRemotePlugin {
    pub port: u16,
    pub render_port: RenderPort,
}

impl NpcRemotePlugin {
    /// Main server on `port`, render server on `port + 1`.
    pub fn new(port: u16) -> Self {
        Self {
            port,
            render_port: RenderPort::Next,
        }
    }
}

impl Default for NpcRemotePlugin {
    fn default() -> Self {
        Self::new(DEFAULT_PORT)
    }
}

fn port_error(what: &str, port: u16, e: &std::io::Error) -> String {
    let msg = format!(
        "{what} port {port} is already in use or cannot be bound ({e}); its server is not \
         started (choose another with --remote-port, or stop the other process)"
    );
    error!(target: "npc_tools", "{msg}");
    msg
}

impl NpcRemotePlugin {
    /// Adds `RemoteHttpPlugin` after probing both ports.
    fn add_http(&self, app: &mut App) -> RemoteStatus {
        let mut status = RemoteStatus {
            port: self.port,
            listening: false,
            render_port: None,
            errors: Vec::new(),
        };
        if let Err(e) = probe_port(self.port) {
            status.errors.push(port_error("BRP", self.port, &e));
            return status;
        }
        let has_render_app = app.get_sub_app_mut(RenderApp).is_some();
        let render_port = match self.render_port.resolve(self.port) {
            Some(p) if has_render_app && p == self.port => {
                status.errors.push(format!(
                    "render BRP port {p} equals the main BRP port; render server not started"
                ));
                None
            }
            Some(p) if has_render_app => match probe_port(p) {
                Ok(()) => Some(p),
                Err(e) => {
                    status.errors.push(port_error("render BRP", p, &e));
                    None
                }
            },
            _ => None,
        };
        let http = RemoteHttpPlugin::default().with_port(self.port);
        match render_port {
            Some(p) => {
                app.add_plugins(http);
                if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
                    render_app.insert_resource(HostPort(p));
                }
            }
            None => {
                // `RemoteHttpPlugin` only sets up the render server when it
                // finds the render sub-app: hide it while the plugin builds.
                let render_app = app.remove_sub_app(RenderApp);
                app.add_plugins(http);
                if let Some(render_app) = render_app {
                    app.insert_sub_app(RenderApp, render_app);
                }
            }
        }
        status.listening = true;
        status.render_port = render_port;
        status
    }
}

impl Plugin for NpcRemotePlugin {
    fn build(&self, app: &mut App) {
        let remote = RemotePlugin::default()
            .with_method_main("npc/pause", methods::pause)
            .with_method_main("npc/resume", methods::resume)
            .with_method_main("npc/step", methods::step)
            .with_method_main("npc/speed", methods::speed)
            .with_method_main("npc/state", methods::state)
            .with_method_main("npc/unit", methods::unit)
            .with_method_main("npc/command", methods::command)
            .with_method_main("npc/journal", methods::journal)
            .with_method_main("npc/perf", methods::perf)
            .with_method_main("npc/perf/reset", methods::perf_reset)
            .with_method_main("npc/record/start", methods::record_start)
            .with_method_main("npc/record/stop", methods::record_stop)
            .with_method_main("npc/methods", methods::methods);
        #[cfg(feature = "rules")]
        let remote = remote.with_method_main("npc/sheet", methods::sheet);
        if !app.is_plugin_added::<RecordingPlugin>() {
            app.add_plugins(RecordingPlugin);
        }
        app.init_resource::<crate::out_path::OutputRoot>();
        app.add_plugins(remote);
        let status = self.add_http(app);
        app.insert_resource(status);
    }
}

/// Live replay recording without the remote interface (added by
/// [`NpcRemotePlugin`]): the [`Recording`] resource, checkpoints in `Last`
/// and writing [`RecordOnExit`] when the app exits. Start it with
/// [`start_recording`].
pub struct RecordingPlugin;

impl Plugin for RecordingPlugin {
    fn build(&self, app: &mut App) {
        // After `ExitSystems` (`exit_on_all_closed` writes `AppExit` in
        // `Last`), so closing the window still writes the replay.
        app.init_resource::<Recording>().add_systems(
            Last,
            (methods::checkpoint_system, methods::record_on_exit_system)
                .chain()
                .after(bevy::window::ExitSystems),
        );
    }
}

/// Registering extra BRP methods after [`NpcRemotePlugin`] (e.g. the game's
/// `npc/screenshot`, which needs the renderer).
pub trait RemoteAppExt {
    /// Adds (or replaces) the main-world method `name`. The handler has the
    /// same shape as a built-in one: `fn(In<Option<Value>>, ..) -> BrpResult`.
    ///
    /// # Panics
    /// When `RemotePlugin` (via [`NpcRemotePlugin`]) is not installed yet.
    fn add_remote_method<M>(
        &mut self,
        name: impl Into<String>,
        handler: impl IntoSystem<In<Option<Value>>, BrpResult, M> + 'static,
    ) -> &mut Self;

    /// Adds (or replaces) the main-world *watching* method `name`: Bevy runs
    /// the handler once per frame (in `RemoteLast`) until it returns
    /// `Ok(Some(..))` or an error; `Ok(None)` means "nothing yet". Over HTTP a
    /// name containing `+watch` streams every `Some` (server-sent events);
    /// any other name gets the first `Some` as its one response, which makes
    /// a non-blocking long-poll.
    ///
    /// # Panics
    /// When `RemotePlugin` (via [`NpcRemotePlugin`]) is not installed yet.
    fn add_remote_watching_method<M>(
        &mut self,
        name: impl Into<String>,
        handler: impl IntoSystem<In<Option<Value>>, BrpResult<Option<Value>>, M> + 'static,
    ) -> &mut Self;
}

impl RemoteAppExt for App {
    fn add_remote_method<M>(
        &mut self,
        name: impl Into<String>,
        handler: impl IntoSystem<In<Option<Value>>, BrpResult, M> + 'static,
    ) -> &mut Self {
        let world = self.world_mut();
        let id = world.register_system(handler);
        world
            .get_resource_mut::<RemoteMethods>()
            .expect("add_remote_method needs NpcRemotePlugin first")
            .insert(name, RemoteMethodSystemId::Instant(id));
        self
    }

    fn add_remote_watching_method<M>(
        &mut self,
        name: impl Into<String>,
        handler: impl IntoSystem<In<Option<Value>>, BrpResult<Option<Value>>, M> + 'static,
    ) -> &mut Self {
        let world = self.world_mut();
        let id = world.register_system(handler);
        world
            .get_resource_mut::<RemoteMethods>()
            .expect("add_remote_watching_method needs NpcRemotePlugin first")
            .insert(name, RemoteMethodSystemId::Watching(id));
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn free_port() -> u16 {
        std::net::TcpListener::bind("127.0.0.1:0")
            .and_then(|l| l.local_addr())
            .expect("free port")
            .port()
    }

    #[test]
    fn render_port_resolves() {
        assert_eq!(RenderPort::Next.resolve(15702), Some(15703));
        assert_eq!(RenderPort::Next.resolve(u16::MAX), None);
        assert_eq!(RenderPort::Port(9000).resolve(15702), Some(9000));
        assert_eq!(RenderPort::Off.resolve(15702), None);
    }

    #[test]
    fn busy_port_is_reported_and_not_started() {
        let taken = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = taken.local_addr().expect("addr").port();
        assert!(probe_port(port).is_err(), "probe sees the taken port");
        let mut app = App::new();
        app.add_plugins(NpcRemotePlugin::new(port));
        let status = app.world().resource::<RemoteStatus>().clone();
        assert!(!status.listening, "{status:?}");
        assert_eq!(status.render_port, None);
        assert_eq!(status.errors.len(), 1, "{status:?}");
        assert!(status.errors[0].contains(&port.to_string()), "{status:?}");
        assert!(
            app.world().get_resource::<HostPort>().is_none(),
            "no HTTP server is set up on a taken port"
        );
        // The in-process methods are still registered.
        assert!(app.world().contains_resource::<RemoteMethods>());
    }

    #[test]
    fn free_port_without_render_app() {
        let port = free_port();
        let mut app = App::new();
        app.add_plugins(NpcRemotePlugin::new(port));
        let status = app.world().resource::<RemoteStatus>().clone();
        assert!(status.listening && status.errors.is_empty(), "{status:?}");
        // Headless: no render sub-app, so no render server.
        assert_eq!(status.render_port, None);
        assert_eq!(app.world().resource::<HostPort>().0, port);
    }

    #[test]
    fn render_port_follows_main_port_or_is_off() {
        let busy = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let busy_port = busy.local_addr().expect("addr").port();
        let other = free_port();
        for render in [
            RenderPort::Next,
            RenderPort::Off,
            RenderPort::Port(other),
            RenderPort::Port(busy_port),
        ] {
            let port = free_port();
            let next_free = port.checked_add(1).filter(|&p| probe_port(p).is_ok());
            let mut app = App::new();
            // A bare render sub-app: enough for RemotePlugin's render side.
            let mut render_app = bevy::app::SubApp::new();
            render_app.init_resource::<bevy::render::RenderScheduleOrder>();
            app.insert_sub_app(RenderApp, render_app);
            app.add_plugins(NpcRemotePlugin {
                port,
                render_port: render,
            });
            let status = app.world().resource::<RemoteStatus>().clone();
            let expected = match render {
                RenderPort::Next => next_free,
                RenderPort::Port(p) if p == other => Some(other),
                _ => None,
            };
            assert!(status.listening, "{render:?}: {status:?}");
            assert_eq!(status.render_port, expected, "{render:?}: {status:?}");
            assert_eq!(
                status.errors.len(),
                usize::from(render == RenderPort::Port(busy_port))
                    + usize::from(render == RenderPort::Next && next_free.is_none()),
                "{render:?}: {status:?}"
            );
            let render_world = app.sub_app(RenderApp).world();
            assert_eq!(
                render_world.get_resource::<HostPort>().map(|p| p.0),
                expected,
                "{render:?}: the render app's server port"
            );
        }
    }
}
