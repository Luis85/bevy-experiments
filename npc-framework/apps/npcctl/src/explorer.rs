//! `npcctl serve --explorer-home DIR --agent NAME`: a detached explorer
//! world (spec G7, plan Task 7). The world is built or resumed by
//! `testbed_content::explorer::host` (no window, no renderer) and hosted by
//! the session server ([`Session::start_app_on`]): the same agent methods
//! as the game window, on a port the agent's folder names.
//!
//! On start it prints one line, `{"listening","port","agent","world",
//! "prompt"}`, writes the agent folder's workspace for this URL and
//! `run.json` (mode `detached`), and runs the sim (`--paused`: paused).
//! It saves every 60 s of wall time (when the tick moved) and on stop
//! (`session/stop`, or Ctrl+C), then removes `run.json` and exits 0. A
//! save that fails on `session/stop` refuses the stop: the world keeps
//! running (spec §8, "Handover"). `world/reset {seed?}` keeps the old save
//! as `<world>.world.<date>.json` and rebuilds the explorer world (the notes
//! resources with it); `world/load_scenario` is refused. A save from another
//! build exits [`EXIT_NEEDS_CHOICE`] (3, as `actor_testbed`). `--wait-port`
//! (with `--resume --port N`) is a handover from the window: it waits up to
//! 10 s for `N` and for the window to exit, never another port
//! ([`EXIT_PORT_IN_USE`], 4).
//!
//! The wall clock is read here (the New seed, the dates, the autosave
//! interval), never in the sim: the sim only sees the seed.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use clap::Args;
use npc_framework::agent_io::http::HttpServer;
use npc_framework::agent_io::session::{Session, SessionOptions};
use npc_framework::bevy_app::App;
use npc_framework::prelude::{SimClock, SimControl};
use npc_framework::tools::output::{EXIT_OK, Failure, Out};
use serde::Serialize;
use serde_json::json;
use testbed_content::TestbedIds;
use testbed_content::explorer::home::{AgentDir, slug};
use testbed_content::explorer::host::{
    self, ExplorerSession, HANDOVER_WAIT, Handover, HostArgs, Mismatch, PORT_SPAN, RunFile,
    RunMode, build_world_with, insert_notes, pick_port, reset_world, rfc3339, save_now,
    wait_for_handover_until, write_session_files,
};

/// The first port an explorer world tries without `--port`.
pub const FIRST_PORT: u16 = 15702;
/// Seconds between autosaves.
pub const AUTOSAVE_SECS: u64 = 60;
/// The exit code for a save from another build without `--load-anyway` /
/// `--fresh-world` (the same as `actor_testbed`'s).
pub const EXIT_NEEDS_CHOICE: u8 = 3;
/// The exit code when `--wait-port` found the port still taken (the same as
/// `actor_testbed --exact-port`'s).
pub const EXIT_PORT_IN_USE: u8 = 4;

/// The explorer flags of `serve`.
#[derive(Args, Debug)]
pub struct ExplorerServeArgs {
    /// Host an explorer world (detached, no window) for one AI agent: the
    /// agents folder (the agent's folder is DIR/<slug(NAME)>).
    #[arg(long, value_name = "DIR", conflicts_with_all = ["scenario", "record", "seed", "token"])]
    pub explorer_home: Option<PathBuf>,
    /// The agent's name.
    #[arg(long, value_name = "NAME", requires = "explorer_home")]
    pub agent: Option<String>,
    /// The new world's name (New only: a resumed world comes from the
    /// agent's folder).
    #[arg(
        long,
        value_name = "NAME",
        requires = "explorer_home",
        conflicts_with = "resume"
    )]
    pub world: Option<String>,
    /// Resume the agent's world from its save instead of creating the agent.
    #[arg(long, requires = "explorer_home")]
    pub resume: bool,
    /// A save from another build: load what still fits.
    #[arg(long, requires = "resume", conflicts_with = "fresh_world")]
    pub load_anyway: bool,
    /// A save from another build: back it up, start a fresh world, keep the notes.
    #[arg(long, requires = "resume")]
    pub fresh_world: bool,
    /// The actor preset of a new agent, by id (`human`, the default, or any
    /// valid preset file `<id>.json` in the actors folder); a resume keeps
    /// the agent's own.
    #[arg(long, value_name = "P", requires = "explorer_home")]
    pub preset: Option<String>,
    /// The actor presets folder (default: `npc-actors` beside the agents
    /// folder; the built-ins are copied into an empty one).
    #[arg(long, value_name = "DIR", requires = "explorer_home")]
    pub actors_dir: Option<PathBuf>,
    /// Start (and reset) the world paused (`npc/resume` runs it).
    #[arg(long, requires = "explorer_home")]
    pub paused: bool,
    /// A handover (the window's "keep it running detached", F9, or the
    /// menu's restart): with `--resume --port N`, wait up to 10 s, in one
    /// budget, for port N to come free and for the agent's other host (the
    /// closing window) to exit; never another port (exit 4, "port N is still
    /// in use").
    #[arg(long, requires_all = ["resume", "port"])]
    pub wait_port: bool,
    /// Autosave interval in seconds of wall time (tests).
    #[arg(long, hide = true, default_value_t = AUTOSAVE_SECS, requires = "explorer_home")]
    pub autosave_secs: u64,
}

/// The listening line, in this key order.
#[derive(Serialize)]
struct Listening<'a> {
    listening: &'a str,
    port: u16,
    agent: &'a str,
    world: &'a str,
    /// The prompt file; `null` when the agent folder could not be written.
    prompt: Option<String>,
}

/// The host's arguments, checked.
fn host_args(e: &ExplorerServeArgs) -> Result<HostArgs, Failure> {
    let home = e
        .explorer_home
        .clone()
        .ok_or_else(|| Failure::usage("give --scenario FILE or --explorer-home DIR"))?;
    let agent = match &e.agent {
        Some(a) if !a.trim().is_empty() => a.clone(),
        Some(_) => return Err(Failure::usage("--agent: the name is blank")),
        None => return Err(Failure::usage("--explorer-home needs --agent NAME")),
    };
    if e.world.as_ref().is_some_and(|w| w.trim().is_empty()) {
        return Err(Failure::usage("--world: the name is blank"));
    }
    if !e.resume && e.world.is_none() {
        return Err(Failure::usage(
            "a new agent needs --world NAME (or --resume an existing one)",
        ));
    }
    let on_mismatch = if e.load_anyway {
        Some(Mismatch::LoadAnyway)
    } else if e.fresh_world {
        Some(Mismatch::FreshWorld)
    } else {
        None
    };
    Ok(HostArgs {
        home,
        agent,
        world: e.world.clone(),
        resume: e.resume,
        on_mismatch,
        preset: e.preset.clone(),
        actors_dir: e.actors_dir.clone(),
    })
}

/// A world seed from the wall clock (app code: the sim only sees the number).
fn wall_clock_seed() -> u64 {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let low = u64::try_from(nanos & u128::from(u64::MAX)).unwrap_or_default();
    low ^ u64::from(std::process::id()).rotate_left(32)
}

/// [`rfc3339`] of now.
fn now_rfc3339() -> String {
    rfc3339(SystemTime::now())
}

/// `--port N` exactly (0: any free port); without it the first port from
/// [`FIRST_PORT`] on whose next port is free too, so the world can later
/// move into the window on the same port.
fn bind(port: Option<u16>) -> Result<HttpServer, String> {
    if let Some(p) = port {
        return HttpServer::bind(p).map_err(|e| format!("cannot listen on 127.0.0.1:{p}: {e}"));
    }
    let last = FIRST_PORT + PORT_SPAN;
    (FIRST_PORT..=last)
        .filter(|&p| pick_port(p, p) == Some(p))
        .find_map(|p| HttpServer::bind(p).ok())
        .ok_or_else(|| {
            format!(
                "no free port in {FIRST_PORT}..={last} (each also needs the next port free, for \
                 the window): stop another game or agent first, or pass --port N"
            )
        })
}

/// Refuses a second host for an agent whose world runs (its `run.json`
/// names a live process).
fn refuse_running(args: &HostArgs) -> Result<(), Failure> {
    let dir = AgentDir {
        dir: args.home.join(slug(&args.agent)),
    };
    match RunFile::read_live(&dir) {
        Some(run) if run.pid != std::process::id() => {
            let mode = match run.mode {
                RunMode::Window => "window",
                RunMode::Detached => "detached",
            };
            Err(Failure::usage(format!(
                "agent {} is already running (pid {}, mode {mode}, port {}): stop it first, \
                 or delete {} if no game runs it",
                args.agent,
                run.pid,
                run.port,
                RunFile::path(&dir).display()
            ))
            .with("running", run))
        }
        _ => Ok(()),
    }
}

/// `--wait-port`: the port and the old host's exit, within
/// [`HANDOVER_WAIT`]. A host still running after the wait is refused by
/// [`refuse_running`] next.
fn wait_for_old_host(args: &HostArgs, port: Option<u16>) -> Result<(), Failure> {
    let port = port
        .filter(|&p| p != 0)
        .ok_or_else(|| Failure::usage("--wait-port needs --port N (not 0)"))?;
    let dir = AgentDir {
        dir: args.home.join(slug(&args.agent)),
    };
    let deadline = Instant::now() + HANDOVER_WAIT;
    match wait_for_handover_until(&dir, port, || Instant::now() >= deadline) {
        Handover::PortInUse => {
            let mut failure = Failure::usage(format!("port {port} is still in use"));
            failure.code = EXIT_PORT_IN_USE;
            Err(failure)
        }
        Handover::Ready | Handover::Running(_) => Ok(()),
    }
}

/// The agent's unit in `app`.
fn unit_of(app: &App, agent: &str) -> Result<npc_framework::prelude::SimId, String> {
    app.world()
        .resource::<TestbedIds>()
        .get(agent)
        .ok_or_else(|| format!("no unit named {agent}"))
}

fn set_paused(app: &mut App, paused: bool) {
    let world = app.world_mut();
    match world.get_resource_mut::<SimControl>() {
        Some(mut c) => c.paused = paused,
        None => world.insert_resource(SimControl {
            paused,
            ..Default::default()
        }),
    }
}

pub fn cmd_serve_explorer(a: &crate::agent::ServeArgs, out: &mut Out) -> Result<u8, Failure> {
    let e = &a.explorer;
    let args = host_args(e)?;
    if e.wait_port {
        wait_for_old_host(&args, a.port)?;
    }
    if args.resume {
        refuse_running(&args)?;
    }
    // Bound once the world is built, before anything is written or
    // loaded: a taken port leaves no folder behind.
    let server: RefCell<Option<HttpServer>> = RefCell::new(None);
    let ready = |_: &App| {
        *server.borrow_mut() = Some(bind(a.port)?);
        Ok(())
    };
    let now = now_rfc3339();
    let (mut app, start, dir) = build_world_with(&args, wall_clock_seed(), &now, |_| {}, ready)
        .map_err(|err| {
            let needs_choice = host::needs_choice(&err);
            let mut failure = Failure::usage(err).with("needs_choice", needs_choice);
            if needs_choice {
                // As `actor_testbed`: the menu offers Load anyway / Fresh world.
                failure.code = EXIT_NEEDS_CHOICE;
            }
            failure
        })?;
    let server = server
        .into_inner()
        .ok_or_else(|| Failure::usage("no server bound"))?;
    let (url, port) = (server.url(), server.port());
    let npcctl = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("npcctl"));
    let files = write_session_files(app.world(), &start, &dir, &url, npcctl, false)
        .map_err(Failure::usage)?;
    if let Some(err) = &files.write_error {
        tracing::warn!("npcctl serve: {err}");
    }
    insert_notes(app.world_mut(), files.unit, &dir, now_rfc3339);
    set_paused(&mut app, e.paused);
    let session = app.world().resource::<ExplorerSession>().clone();
    let run = RunFile {
        pid: std::process::id(),
        port,
        mode: RunMode::Detached,
        started: now,
        agent: session.meta.agent.clone(),
        world: session.meta.world.clone(),
    };
    if let Err(err) = run.write(&dir) {
        tracing::warn!(
            "npcctl serve: cannot write {}: {err}",
            RunFile::path(&dir).display()
        );
    }
    let prompt = files
        .write_error
        .is_none()
        .then(|| files.path.display().to_string());
    out.json(&Listening {
        listening: &url,
        port,
        agent: &session.meta.agent,
        world: &session.meta.world,
        prompt,
    })?;
    // The first line must reach a waiting client before the loop starts.
    out.finish()?;

    let stop = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&stop);
    if let Err(err) = ctrlc::set_handler(move || flag.store(true, Ordering::SeqCst)) {
        tracing::warn!("npcctl serve: Ctrl+C will not save ({err}); use `npcctl session stop`");
    }
    let start_tick = app.world().resource::<SimClock>().tick;
    let stopped_at: Rc<Cell<Option<u64>>> = Rc::new(Cell::new(None));
    let options = SessionOptions {
        tick_hz: a.tick_hz,
        seed: Some(session.seed),
        on_frame: Some(autosave(
            dir.clone(),
            Duration::from_secs(e.autosave_secs.max(1)),
            start_tick,
        )),
        on_stop: Some(save_on_stop(dir.clone(), Rc::clone(&stopped_at))),
        rebuild: Some(rebuild(session.clone(), dir.clone(), e.paused)),
        stop: Some(stop),
        ..SessionOptions::default()
    };
    let result = Session::start_app_on(app, server, options).and_then(Session::run);
    if let Err(err) = run.remove(&dir) {
        tracing::warn!(
            "npcctl serve: cannot remove {}: {err}",
            RunFile::path(&dir).display()
        );
    }
    result.map_err(Failure::usage)?;
    out.json(&json!({
        "stopped": true,
        "tick": stopped_at.get(),
        "saved": session.save_path().display().to_string(),
    }))?;
    Ok(EXIT_OK)
}

/// `on_frame`: saves every `every` of wall time when the tick moved since
/// the last save; a failure is a warning (the next interval tries again).
fn autosave(
    dir: AgentDir,
    every: Duration,
    start_tick: u64,
) -> Box<dyn FnMut(&mut npc_framework::bevy_ecs::world::World)> {
    let mut last = Instant::now();
    let mut saved_tick = start_tick;
    Box::new(move |world| {
        if last.elapsed() < every {
            return;
        }
        last = Instant::now();
        let tick = world.resource::<SimClock>().tick;
        if tick == saved_tick {
            return;
        }
        match save_now(world, &dir, &now_rfc3339()) {
            Ok(()) => {
                saved_tick = tick;
                tracing::info!("npcctl serve: autosave at tick {tick}");
            }
            Err(err) => tracing::warn!("npcctl serve: autosave failed: {err}"),
        }
    })
}

/// `on_stop`: the final save; its failure refuses `session/stop`.
fn save_on_stop(
    dir: AgentDir,
    stopped_at: Rc<Cell<Option<u64>>>,
) -> npc_framework::agent_io::session::StopHook {
    Box::new(move |world| {
        let tick = world.resource::<SimClock>().tick;
        save_now(world, &dir, &now_rfc3339()).map_err(|err| format!("the save failed: {err}"))?;
        stopped_at.set(Some(tick));
        let saved = world
            .get_resource::<ExplorerSession>()
            .map(|s| s.save_path().display().to_string());
        Ok(json!({"tick": tick, "saved": saved}))
    })
}

/// `world/reset {seed?}`: the explorer world rebuilt for `seed`, with its
/// notes resources (Ruling R6) and the `--paused` state. The old save is
/// kept first, as `--fresh-world` keeps it (`<world>.world.<date>.json`),
/// and the reset world saved at tick 0; a failed save puts the old one back
/// and refuses the reset.
fn rebuild(
    session: ExplorerSession,
    dir: AgentDir,
    paused: bool,
) -> npc_framework::agent_io::session::RebuildHook {
    Box::new(move |seed| {
        let mut app = reset_world(&session, seed, |_| {})?;
        let unit = unit_of(&app, &session.meta.agent)?;
        let (world, save, now) = (&session.meta.world, session.save_path(), now_rfc3339());
        let backup = if save.exists() {
            Some(
                dir.backup_save(world, &now)
                    .map_err(|e| format!("cannot back up the save {}: {e}", save.display()))?,
            )
        } else {
            None
        };
        if let Err(e) = save_now(app.world_mut(), &dir, &now) {
            let Some(backup) = &backup else {
                return Err(format!("{e}; the world was not reset"));
            };
            return Err(match std::fs::rename(backup, &save) {
                Ok(()) => format!("{e}; the world was not reset"),
                Err(back) => format!(
                    "{e}; the world was not reset, and the old save could not be put back \
                     ({back}): it is kept as {} (rename it to {} to resume it)",
                    backup.display(),
                    save.display()
                ),
            });
        }
        if let Some(backup) = &backup {
            tracing::info!(
                "npcctl serve: world/reset kept the old save as {}",
                backup.display()
            );
        }
        insert_notes(app.world_mut(), unit, &dir, now_rfc3339);
        set_paused(&mut app, paused);
        Ok(app)
    })
}
