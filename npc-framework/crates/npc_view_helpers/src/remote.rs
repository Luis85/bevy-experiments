//! The remote interface of a windowed host, shared by `apps/village` and
//! `apps/actor_testbed`: the command-line flags ([`RemoteArgs`], always
//! built, so an app without BRP can refuse `--remote` clearly) and, with
//! feature `remote`, Bevy's BRP with the `npc/*` methods
//! (`install_remote`), the window's `npc/screenshot {path}` method,
//! `--record FILE` (`record_from_startup`) and the launcher's graceful quit
//! `app/quit {pid?}` (`install_app_quit`: answers, then exits 0 through the
//! app's ordinary exit; registered by apps only, never by a library plugin).
//!
//! **Off by default in release builds**: an app passes
//! `cfg!(debug_assertions)` to [`RemoteArgs::new`], so a release binary only
//! listens with `--remote`. `bevy_remote` 0.20 cannot refuse cross-origin or
//! DNS-rebinding requests (no request hook; `npc_tools::remote` docs), so the
//! port is a development tool: use `--no-remote` outside development.
//!
//! Client file paths (`npc/screenshot {path}`, `npc/record/stop {path}`) are
//! relative and confined under the output root (`--out-dir`, default
//! `<cwd>/target/npc-out`), checked by `npc_tools::out_path`.

use std::path::PathBuf;

#[cfg(feature = "remote")]
use bevy::prelude::*;
#[cfg(feature = "remote")]
use npc_tools::out_path::OutputRoot;
#[cfg(feature = "remote")]
use npc_tools::remote::{
    BrpError, BrpResult, NpcRemotePlugin, RecordOnExit, RecordingPlugin, RemoteAppExt,
    RemoteStatus, RenderPort, error_codes, start_recording,
};
#[cfg(feature = "remote")]
use serde_json::{Value, json};

/// Checkpoint interval of `--record`, in ticks.
pub const RECORD_EVERY: u64 = 100;
/// Bevy's default BRP port.
pub const DEFAULT_PORT: u16 = 15702;

/// The remote flags of a windowed app.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemoteArgs {
    /// Serve BRP (`--remote` / `--no-remote`).
    pub remote: bool,
    /// The main BRP port (`--remote-port`, default 15702).
    pub port: u16,
    /// `None`: `port + 1`; `Some(None)`: off (`--no-render-remote`);
    /// `Some(Some(p))`: `p` (`--render-remote-port`).
    pub render_port: Option<Option<u16>>,
    /// The output root of client paths (`--out-dir`; default
    /// `<cwd>/target/npc-out`).
    pub out_dir: Option<PathBuf>,
}

impl RemoteArgs {
    /// The usage fragment of these flags.
    pub const USAGE: &'static str = "[--remote | --no-remote] [--remote-port N] \
        [--render-remote-port N | --no-render-remote] [--out-dir DIR]";

    /// Defaults; `remote_by_default` is the app's `cfg!(debug_assertions)`
    /// (on in dev builds, off in release builds).
    pub fn new(remote_by_default: bool) -> Self {
        Self {
            remote: remote_by_default,
            port: DEFAULT_PORT,
            render_port: None,
            out_dir: None,
        }
    }

    /// A flag without a value; `true` when it was one of ours.
    pub fn flag(&mut self, flag: &str) -> bool {
        match flag {
            "--remote" => self.remote = true,
            "--no-remote" => self.remote = false,
            "--no-render-remote" => self.render_port = Some(None),
            _ => return false,
        }
        true
    }

    /// A flag with a value: `None` when it is not ours, else whether
    /// `value` was valid (`Err` names the problem).
    pub fn value(&mut self, flag: &str, value: &str) -> Option<Result<(), String>> {
        let port = || {
            value
                .parse::<u16>()
                .map_err(|_| format!("{flag}: `{value}` is not a port"))
        };
        Some(match flag {
            "--remote-port" => port().map(|p| self.port = p),
            "--render-remote-port" => port().map(|p| self.render_port = Some(Some(p))),
            "--out-dir" => {
                self.out_dir = Some(PathBuf::from(value));
                Ok(())
            }
            _ => return None,
        })
    }

    #[cfg(feature = "remote")]
    fn render(&self) -> RenderPort {
        match self.render_port {
            None => RenderPort::Next,
            Some(None) => RenderPort::Off,
            Some(Some(p)) => RenderPort::Port(p),
        }
    }
}

/// Installs BRP per `args` (nothing when `args.remote` is off): the
/// [`OutputRoot`], `NpcRemotePlugin` and `npc/screenshot {path,
/// overwrite?}`. Returns whether the remote plugin was added, so the app can
/// add its own methods after it (e.g. `npc_agent_io`'s `AgentRemotePlugin`).
/// `name` prefixes the log lines.
#[cfg(feature = "remote")]
pub fn install_remote(app: &mut App, name: &str, args: &RemoteArgs) -> bool {
    if !args.remote {
        info!("{name}: remote interface off (--remote turns it on)");
        return false;
    }
    let root = args
        .out_dir
        .as_ref()
        .map_or_else(OutputRoot::default, OutputRoot::new);
    let root_dir = root.0.clone();
    app.insert_resource(root)
        .add_plugins(NpcRemotePlugin {
            port: args.port,
            render_port: args.render(),
        })
        .add_remote_method("npc/screenshot", screenshot);
    // A taken port was already logged as an error by the plugin.
    let status = app.world().resource::<RemoteStatus>();
    if status.listening {
        info!(
            "{name}: remote interface on http://127.0.0.1:{}/ (development only; no \
             Origin/Host checks on BRP), render-app server {}, client files under {}",
            status.port,
            status
                .render_port
                .map_or_else(|| "off".to_owned(), |p| format!("on port {p}")),
            root_dir.display()
        );
    }
    true
}

/// `--record FILE`: records from the first frame (after every `Startup`
/// system, so the spawned world is in the first checkpoint) and writes the
/// replay to `path` on exit.
#[cfg(feature = "remote")]
pub fn record_from_startup(app: &mut App, name: &'static str, path: PathBuf) {
    if !app.is_plugin_added::<RecordingPlugin>() {
        app.add_plugins(RecordingPlugin);
    }
    app.insert_resource(RecordOnExit(path))
        .add_systems(PostStartup, move |world: &mut World| {
            if let Err(e) = start_recording(world, RECORD_EVERY) {
                error!("{name}: --record: {}", e.message);
            }
        });
}

/// The graceful quit method a launcher calls (`app/quit {pid?}`).
pub const APP_QUIT: &str = "app/quit";

/// `app/quit`'s exit waits this long after the request, so its answer
/// leaves before the process ends.
pub const QUIT_DELAY: std::time::Duration = std::time::Duration::from_millis(200);

/// An `app/quit` that was answered: [`quit_when_due`] writes the
/// `AppExit` once [`QUIT_DELAY`] has passed.
#[cfg(feature = "remote")]
#[derive(Resource, Clone, Copy, Debug)]
pub struct QuitRequested {
    /// When the request came.
    pub at: std::time::Instant,
}

/// `app/quit`'s exit: the [`quit_when_due`] system. Only apps add it
/// (through [`install_app_quit`]): a library never exits the host.
#[cfg(feature = "remote")]
pub struct AppQuitPlugin;

#[cfg(feature = "remote")]
impl Plugin for AppQuitPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, quit_when_due);
    }
}

/// Registers `app/quit` on the app's BRP (call after [`install_remote`]
/// returned true). Apps only.
#[cfg(feature = "remote")]
pub fn install_app_quit(app: &mut App) {
    app.add_plugins(AppQuitPlugin)
        .add_remote_method(APP_QUIT, app_quit);
}

/// `app/quit {pid?}` → `{"quitting": true, "pid": N}`: answers, then the
/// app exits [`QUIT_DELAY`] later through an ordinary `AppExit::Success`
/// ([`quit_when_due`]; BRP handlers run after `Last`, so exiting right here
/// would skip the app's exit systems and might cut the answer off). A
/// launcher passes the pid it started: a request that reaches another
/// process on the port is refused.
///
/// # Errors
/// Bad params, or a `pid` that is not this process's.
#[cfg(feature = "remote")]
pub fn app_quit(In(params): In<Option<Value>>, world: &mut World) -> BrpResult {
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct QuitParams {
        pid: Option<u32>,
    }
    let params: QuitParams = match params {
        None | Some(Value::Null) => QuitParams { pid: None },
        Some(p) => serde_json::from_value(p).map_err(|e| invalid(format!("bad params: {e}")))?,
    };
    let me = std::process::id();
    if let Some(pid) = params.pid.filter(|&p| p != me) {
        return Err(invalid(format!(
            "app/quit names pid {pid}, but this process is pid {me}: not quitting"
        )));
    }
    if world.get_resource::<QuitRequested>().is_none() {
        info!("app/quit: exiting in {} ms", QUIT_DELAY.as_millis());
        world.insert_resource(QuitRequested {
            at: std::time::Instant::now(),
        });
    }
    Ok(json!({"quitting": true, "pid": me}))
}

/// `Update`: writes `AppExit::Success` once an answered `app/quit` is
/// [`QUIT_DELAY`] old (so the app's own exit systems, e.g. a final save in
/// `Last`, run as for any exit).
#[cfg(feature = "remote")]
pub fn quit_when_due(world: &mut World) {
    if world
        .get_resource::<QuitRequested>()
        .is_some_and(|q| q.at.elapsed() >= QUIT_DELAY)
    {
        world.write_message(AppExit::Success);
    }
}

#[cfg(feature = "remote")]
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ScreenshotParams {
    path: String,
    #[serde(default)]
    overwrite: bool,
}

#[cfg(feature = "remote")]
fn invalid(message: String) -> BrpError {
    BrpError {
        code: error_codes::INVALID_PARAMS,
        message,
        data: None,
    }
}

/// `npc/screenshot {path, overwrite?}` → `{"path": .., "pending": true}`.
/// `path` is relative to the [`OutputRoot`] (`.png` forced, no overwrite
/// unless asked); the PNG is written a frame or two after the call returns.
#[cfg(feature = "remote")]
pub fn screenshot(In(params): In<Option<Value>>, world: &mut World) -> BrpResult {
    let ScreenshotParams { path, overwrite } = params
        .ok_or_else(|| "missing params, expected {\"path\": string}".to_owned())
        .and_then(|p| serde_json::from_value(p).map_err(|e| format!("bad params: {e}")))
        .map_err(invalid)?;
    let root = world.get_resource_or_init::<OutputRoot>().clone();
    let path = root.file(&path, "png", overwrite).map_err(invalid)?;
    crate::save_screenshot(world, path.clone());
    Ok(json!({"path": path, "pending": true}))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_parse() {
        let mut a = RemoteArgs::new(false);
        assert!(!a.remote);
        assert!(a.flag("--remote") && a.remote);
        assert!(a.flag("--no-remote") && !a.remote);
        assert!(a.flag("--no-render-remote"));
        assert_eq!(a.render_port, Some(None));
        assert!(!a.flag("--paused"));
        assert_eq!(a.value("--remote-port", "15712"), Some(Ok(())));
        assert_eq!(a.port, 15712);
        assert!(matches!(a.value("--remote-port", "x"), Some(Err(_))));
        assert_eq!(a.value("--out-dir", "shots"), Some(Ok(())));
        assert_eq!(a.out_dir, Some(PathBuf::from("shots")));
        assert_eq!(a.value("--seed", "1"), None);
        assert_eq!(a.render_port, Some(None));
    }

    #[cfg(feature = "remote")]
    #[test]
    fn screenshot_refuses_paths_outside_the_root() {
        let root = std::env::temp_dir().join(format!("npc-shot-{}", std::process::id()));
        let mut world = World::new();
        world.insert_resource(OutputRoot::new(&root));
        for bad in [
            json!({"path": "/tmp/x.png"}),
            json!({"path": "C:\\x.png"}),
            json!({"path": "../x.png"}),
            json!({"path": "x.png", "extra": 1}),
        ] {
            let err = screenshot(In(Some(bad.clone())), &mut world).expect_err("refused");
            assert_eq!(err.code, error_codes::INVALID_PARAMS, "{bad}");
        }
        let _ = std::fs::remove_dir_all(&root);
    }
}
