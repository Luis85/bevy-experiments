//! Probe: `bevy_remote` custom methods, a runner-thread round trip, and an offscreen screenshot.
#![expect(
    clippy::disallowed_types,
    reason = "probe measures wall-clock stalls with Instant"
)]

use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::thread::ThreadId;
use std::time::{Duration, Instant};

use bevy::camera::RenderTarget;
use bevy::prelude::*;
use bevy::remote::http::RemoteHttpPlugin;
use bevy::remote::{BrpError, BrpResult, RemotePlugin, error_codes};
use bevy::render::render_resource::TextureFormat;
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk};
use bevy::winit::WinitSettings;
use bevy_brp_extras::{AgentTool, AppAgentToolExt, BrpExtrasPlugin};
use crossbeam_channel::{Receiver, Sender};
use serde_json::{Value, json};

const PORT: u16 = 15799;
const PNG_PATH: &str = "target/probe-remote.png";

/// Request to the stand-in runner thread: `(n, artificial delay in ms)`.
type RunnerRequest = (i64, u64);

#[derive(Resource)]
struct RunnerLink {
    tx: Sender<RunnerRequest>,
    rx: Receiver<i64>,
}

#[derive(Resource)]
struct Target(Handle<Image>);

#[derive(Resource, Clone)]
struct Stats {
    frames: Arc<AtomicU64>,
    /// Pixels in the screenshot that differ from the corner (clear colour) pixel.
    cube_px: Arc<AtomicUsize>,
    total_px: Arc<AtomicUsize>,
    /// Whether the last `fl.ping` handler ran on the thread that called `main`.
    ping_on_main: Arc<AtomicBool>,
}

#[derive(Resource)]
struct MainThread(ThreadId);

#[derive(Resource)]
struct Done(Receiver<Result<String, String>>);

/// `fl.ping {n, delay_ms?}`: blocks on the runner thread's reply for at most 2 s.
fn ping(
    In(params): In<Option<Value>>,
    link: Res<RunnerLink>,
    main: Res<MainThread>,
    stats: Res<Stats>,
) -> BrpResult {
    stats
        .ping_on_main
        .store(std::thread::current().id() == main.0, Ordering::SeqCst);
    let p = params.unwrap_or(Value::Null);
    let n = p["n"].as_i64().ok_or_else(|| BrpError {
        code: error_codes::INVALID_PARAMS,
        message: "missing n".into(),
        data: None,
    })?;
    let delay = p["delay_ms"].as_u64().unwrap_or(0);
    link.tx
        .send((n, delay))
        .map_err(|e| internal(e.to_string()))?;
    let r = link
        .rx
        .recv_timeout(Duration::from_secs(2))
        .map_err(|e| internal(e.to_string()))?;
    Ok(json!(r))
}

/// `fl.watch`: watching method; emits the frame counter on every update.
fn watch(In(_): In<Option<Value>>, stats: Res<Stats>) -> BrpResult<Option<Value>> {
    Ok(Some(json!({"frames": stats.frames.load(Ordering::SeqCst)})))
}

fn internal(message: String) -> BrpError {
    BrpError {
        code: error_codes::INTERNAL_ERROR,
        message,
        data: None,
    }
}

/// `fl.screenshot {path}`: screenshot of the offscreen image.
fn screenshot(
    In(params): In<Option<Value>>,
    target: Res<Target>,
    stats: Res<Stats>,
    mut commands: Commands,
) -> BrpResult {
    let path = params
        .as_ref()
        .and_then(|p| p["path"].as_str())
        .unwrap_or(PNG_PATH)
        .to_string();
    let (cube_px, total_px) = (stats.cube_px.clone(), stats.total_px.clone());
    commands
        .spawn(Screenshot::image(target.0.clone()))
        .observe(save_to_disk(path))
        .observe(move |cap: On<ScreenshotCaptured>| {
            if let Some(data) = cap.image.data.as_ref() {
                // The corner pixel is the clear colour; anything else is the rendered cube.
                let px: Vec<&[u8]> = data.chunks(4).collect();
                let diff = px.iter().filter(|p| **p != px[0]).count();
                cube_px.store(diff, Ordering::SeqCst);
                total_px.store(px.len(), Ordering::SeqCst);
            }
        });
    Ok(json!("requested"))
}

fn setup(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let image = images.add(Image::new_target_texture(
        256,
        256,
        TextureFormat::Rgba8UnormSrgb,
        None,
    ));
    commands.spawn((
        Camera3d::default(),
        RenderTarget::Image(image.clone().into()),
        Transform::from_xyz(2.5, 2.0, 3.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(1.0, 1.0, 1.0))),
        MeshMaterial3d(materials.add(Color::srgb(0.8, 0.3, 0.2))),
    ));
    commands.spawn((
        DirectionalLight::default(),
        Transform::from_xyz(3.0, 5.0, 2.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    commands.insert_resource(Target(image));
}

fn count_frames(stats: Res<Stats>) {
    stats.frames.fetch_add(1, Ordering::SeqCst);
}

fn finish(done: Res<Done>, mut exit: MessageWriter<AppExit>) {
    let msg = match done.0.try_recv() {
        Ok(r) => Some(r),
        // The self-test thread died without a result (panic).
        Err(crossbeam_channel::TryRecvError::Disconnected) => {
            Some(Err("self-test thread panicked".to_string()))
        }
        Err(crossbeam_channel::TryRecvError::Empty) => None,
    };
    if let Some(r) = msg {
        match r {
            Ok(line) => {
                println!("{line}");
                exit.write(AppExit::Success);
            }
            Err(e) => {
                eprintln!("remote probe: FAILED {e}");
                exit.write(AppExit::error());
            }
        }
    }
}

fn rpc(method: &str, params: Value) -> Result<Value, String> {
    let body = json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": params}).to_string();
    let mut s = TcpStream::connect(("127.0.0.1", PORT)).map_err(|e| e.to_string())?;
    s.set_read_timeout(Some(Duration::from_secs(10))).ok();
    write!(
        s,
        "POST / HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .map_err(|e| e.to_string())?;
    let mut buf = String::new();
    s.read_to_string(&mut buf).map_err(|e| e.to_string())?;
    // The server answers with `Transfer-Encoding: chunked`; the JSON body is the span from the
    // first `{` to the last `}` (chunk-size lines and the terminator sit outside it).
    let (_, http_body) = buf.split_once("\r\n\r\n").ok_or("no http body")?;
    let json_body = match (http_body.find('{'), http_body.rfind('}')) {
        (Some(a), Some(b)) => &http_body[a..=b],
        _ => return Err(format!("no json in {http_body}")),
    };
    let v: Value = serde_json::from_str(json_body).map_err(|e| format!("{e}: {json_body}"))?;
    if let Some(err) = v.get("error") {
        return Err(format!("{method}: {err}"));
    }
    Ok(v["result"].clone())
}

/// Open a watching request and return the first server-sent event's data line.
fn watch_first_event() -> Result<String, String> {
    let body = json!({"jsonrpc": "2.0", "id": 1, "method": "fl.watch+watch"}).to_string();
    let mut s = TcpStream::connect(("127.0.0.1", PORT)).map_err(|e| e.to_string())?;
    s.set_read_timeout(Some(Duration::from_secs(5))).ok();
    write!(
        s,
        "POST / HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    )
    .map_err(|e| e.to_string())?;
    let mut acc = Vec::new();
    let mut chunk = [0u8; 1024];
    loop {
        let n = s
            .read(&mut chunk)
            .map_err(|e| format!("{e}; got so far: {:?}", String::from_utf8_lossy(&acc)))?;
        if n == 0 {
            return Err(format!("stream closed: {}", String::from_utf8_lossy(&acc)));
        }
        acc.extend_from_slice(&chunk[..n]);
        let text = String::from_utf8_lossy(&acc).to_string();
        if let Some(i) = text.find("data:")
            && let Some(len) = text[i..].find("\n\n")
        {
            return Ok(format!(
                "{} | headers: {}",
                text[i..i + len].trim(),
                text.lines().take(4).collect::<Vec<_>>().join(" / ")
            ));
        }
    }
}

fn self_test(stats: Stats) -> Result<String, String> {
    // Wait for the server to come up.
    let start = Instant::now();
    // Retry only the TCP connect; any RPC-level failure is a real failure.
    while TcpStream::connect(("127.0.0.1", PORT)).is_err() {
        if start.elapsed() > Duration::from_secs(60) {
            return Err("server did not start in 60 s".into());
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    let ping42 = rpc("fl.ping", json!({"n": 41}))?;
    if !stats.ping_on_main.load(Ordering::SeqCst) {
        return Err("fl.ping handler did not run on the main thread".into());
    }
    println!("probe: fl.ping handler ran on the main thread (thread id equal to main()'s)");
    if ping42 != json!(42) {
        return Err(format!("ping returned {ping42}"));
    }
    let disc = rpc("rpc.discover", Value::Null)?.to_string();
    if !disc.contains("fl.ping") {
        return Err("rpc.discover lacks fl.ping".into());
    }
    let tools = rpc("brp_extras/agent_tools", Value::Null)?.to_string();
    if !tools.contains("fl.ping") {
        return Err(format!("agent tool listing lacks fl.ping: {tools}"));
    }
    let w = watch_first_event()?;
    println!("probe: watching method fl.watch first event: {w}");
    println!("probe: discover/agent_tools OK; agent_tools = {tools}");

    // Does a blocking method stall the main schedule? Sample the frame counter while pinging with a delay.
    let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let (st, stp) = (stats.clone(), stop.clone());
    let sampler = std::thread::spawn(move || {
        let (mut last, mut last_t, mut max_gap) = (
            st.frames.load(Ordering::SeqCst),
            Instant::now(),
            Duration::ZERO,
        );
        while !stp.load(Ordering::SeqCst) {
            std::thread::sleep(Duration::from_millis(5));
            let f = st.frames.load(Ordering::SeqCst);
            if f != last {
                max_gap = max_gap.max(last_t.elapsed());
                last = f;
                last_t = Instant::now();
            }
        }
        max_gap
    });
    std::thread::sleep(Duration::from_millis(300));
    let t = Instant::now();
    let r = rpc("fl.ping", json!({"n": 1, "delay_ms": 700}))?;
    let rt = t.elapsed();
    stop.store(true, Ordering::SeqCst);
    let max_gap = sampler.join().map_err(|_| "sampler panicked")?;
    println!("probe: delayed ping={r} took {rt:?}; max gap between main-world frames {max_gap:?}");
    // Timeout path: reply later than 2 s must give an error, not a hang.
    let to = rpc("fl.ping", json!({"n": 1, "delay_ms": 2500}));
    println!("probe: 2.5s-delay ping -> {to:?}");
    match &to {
        Err(e) if e.contains("-32603") => {}
        other => return Err(format!("expected -32603 timeout, got {other:?}")),
    }
    // The worker is still sleeping on the timed-out request; its reply (2) is stale.
    let after = rpc("fl.ping", json!({"n": 10}))?;
    println!(
        "probe: ping n=10 right after the timeout returned {after} (11 = fresh, 2 = stale reply from the timed-out call)"
    );
    if after != json!(2) && after != json!(11) {
        return Err(format!("unexpected ping after timeout: {after}"));
    }

    std::thread::sleep(Duration::from_secs(2));
    let _ = std::fs::remove_file(PNG_PATH);
    rpc("fl.screenshot", json!({"path": PNG_PATH}))?;
    let t = Instant::now();
    let bytes = loop {
        if let Ok(m) = std::fs::metadata(PNG_PATH)
            && m.len() > 0
        {
            std::thread::sleep(Duration::from_millis(300));
            break std::fs::metadata(PNG_PATH).map(|m| m.len()).unwrap_or(0);
        }
        if t.elapsed() > Duration::from_secs(20) {
            return Err("screenshot not written in 20 s".into());
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    let nb = stats.cube_px.load(Ordering::SeqCst);
    let total = stats.total_px.load(Ordering::SeqCst);
    if bytes <= 1024 {
        return Err(format!("png too small: {bytes}"));
    }
    // Some, but not all, pixels differ from the clear colour: the cube was rendered.
    if nb == 0 || nb == total {
        return Err(format!(
            "no cube in screenshot: {nb} of {total} px differ from clear colour"
        ));
    }
    // Idle frame rate of the hidden window (compare default vs continuous WinitSettings).
    let (f0, t0) = (stats.frames.load(Ordering::SeqCst), Instant::now());
    std::thread::sleep(Duration::from_secs(4));
    let f1 = stats.frames.load(Ordering::SeqCst);
    println!(
        "probe: idle hidden-window update rate = {:.1} frames/s ({})",
        (f1 - f0) as f64 / t0.elapsed().as_secs_f64(),
        std::env::args()
            .nth(1)
            .unwrap_or_else(|| "continuous".into())
    );
    Ok(format!(
        "remote probe: OK ping=42 screenshot={bytes} (cube px {nb} of {total})"
    ))
}

fn main() {
    let (req_tx, req_rx) = crossbeam_channel::unbounded::<RunnerRequest>();
    let (rep_tx, rep_rx) = crossbeam_channel::unbounded::<i64>();
    std::thread::spawn(move || {
        while let Ok((n, delay)) = req_rx.recv() {
            if delay > 0 {
                std::thread::sleep(Duration::from_millis(delay));
            }
            if rep_tx.send(n + 1).is_err() {
                break;
            }
        }
    });
    let stats = Stats {
        frames: Arc::default(),
        cube_px: Arc::default(),
        total_px: Arc::default(),
        ping_on_main: Arc::default(),
    };
    let (done_tx, done_rx) = crossbeam_channel::bounded(1);
    let st = stats.clone();
    std::thread::spawn(move || {
        let _ = done_tx.send(self_test(st));
    });

    let mut app = App::new();
    // Keep updating while unfocused/hidden unless the `default` variant is requested.
    if std::env::args().nth(1).as_deref() != Some("default") {
        app.insert_resource(WinitSettings::continuous());
    }
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            visible: false,
            ..default()
        }),
        ..default()
    }))
    .add_plugins(
        RemotePlugin::default()
            .with_method_main("fl.ping", ping)
            .with_method_main("fl.screenshot", screenshot)
            .with_watching_method_main("fl.watch+watch", watch),
    )
    .add_plugins(BrpExtrasPlugin::with_http_plugin(
        RemoteHttpPlugin::default().with_port(PORT),
    ))
    .register_agent_tool(AgentTool::new(
        "fl.ping",
        "fl.ping",
        "Round-trip n to the runner thread; returns n + 1.",
    ))
    .insert_resource(RunnerLink {
        tx: req_tx,
        rx: rep_rx,
    })
    .insert_resource(MainThread(std::thread::current().id()))
    .insert_resource(stats)
    .insert_resource(Done(done_rx))
    .add_systems(Startup, setup)
    .add_systems(Update, (count_frames, finish));
    app.run();
}
