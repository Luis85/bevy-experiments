//! `npcctl serve --explorer-home`: a detached explorer world (spec G7, plan
//! Task 7). Each test spawns its own `npcctl serve` on a free port picked at
//! runtime (`--port 0`), in its own agents folder, and stops it by
//! `session/stop` (a failing test kills only the child it started).

use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdout, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use npc_framework::agent_io::http::{ClientError, RpcClient};
use npc_framework::snapshot;
use serde_json::{Value, json};

/// Generous: a busy machine builds a world in seconds, not milliseconds.
const START: Duration = Duration::from_secs(180);
const EXIT: Duration = Duration::from_secs(120);

fn home(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("npcctl-explorer-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// A spawned `npcctl serve`, killed when dropped (this child only).
struct Served {
    child: Child,
    lines: mpsc::Receiver<String>,
    line: Value,
    client: RpcClient,
}

impl Drop for Served {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Every stdout line of the child, as it comes.
fn lines(stdout: ChildStdout) -> mpsc::Receiver<String> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            let Ok(line) = line else { return };
            if tx.send(line).is_err() {
                return;
            }
        }
    });
    rx
}

/// `serve --explorer-home HOME --actors-dir HOME/actors`: every child gets
/// its own actor presets folder (never the shared default beside the temp
/// folder).
fn serve_home(home: &Path) -> [String; 5] {
    [
        "serve".to_owned(),
        "--explorer-home".to_owned(),
        home.display().to_string(),
        "--actors-dir".to_owned(),
        home.join("actors").display().to_string(),
    ]
}

fn npcctl() -> Command {
    Command::new(env!("CARGO_BIN_EXE_npcctl"))
}

/// `npcctl serve --explorer-home HOME --agent Wren --port 0 ARGS..`; waits
/// for the listening line.
fn serve(home: &Path, args: &[&str]) -> Served {
    let mut child = npcctl()
        .args(serve_home(home))
        .args(["--agent", "Wren", "--port", "0"])
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("serve starts");
    let lines = lines(child.stdout.take().unwrap());
    let first = match lines.recv_timeout(START) {
        Ok(l) => l,
        Err(e) => {
            let _ = child.kill();
            panic!("no listening line: {e}");
        }
    };
    let line: Value = serde_json::from_str(&first).unwrap_or_else(|e| panic!("{e}: {first}"));
    let url = line["listening"]
        .as_str()
        .unwrap_or_else(|| panic!("{line}"))
        .to_owned();
    Served {
        child,
        lines,
        line,
        client: RpcClient::new(&url).unwrap(),
    }
}

impl Served {
    fn call(&self, method: &str, params: Value) -> Value {
        self.client
            .call(method, params)
            .unwrap_or_else(|e| panic!("{method}: {e}"))
    }

    /// `session/stop`, then the exit code and the last stdout line.
    fn stop(mut self) -> (Option<i32>, Value) {
        let reply = self.call("session/stop", json!({}));
        assert_eq!(reply["stopped"], true, "{reply}");
        let deadline = Instant::now() + EXIT;
        let code = loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                break status.code();
            }
            assert!(Instant::now() < deadline, "serve did not exit after stop");
            std::thread::sleep(Duration::from_millis(50));
        };
        let last = self
            .lines
            .recv_timeout(Duration::from_secs(10))
            .unwrap_or_default();
        (code, serde_json::from_str(&last).unwrap_or(Value::Null))
    }

    /// The agent's unit: the one the explorer controller drives.
    fn unit(&self) -> u64 {
        let units = self.call("agent/list_units", json!({}));
        units["units"]
            .as_array()
            .unwrap()
            .iter()
            .find(|u| u["controller"]["id"] == "explorer")
            .unwrap_or_else(|| panic!("no explorer unit: {units}"))["id"]
            .as_u64()
            .unwrap()
    }
}

fn read_json(path: &Path) -> Value {
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    serde_json::from_str(text.trim_start_matches('\u{feff}')).unwrap()
}

#[test]
fn serve_starts_a_new_explorer_world() {
    let home = home("new");
    let s = serve(&home, &["--world", "Moor"]);
    let line = &s.line;
    // The listening line: exactly these keys.
    let mut keys: Vec<&str> = line
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        ["agent", "listening", "port", "prompt", "world"],
        "{line}"
    );
    let port = line["port"].as_u64().unwrap();
    assert_eq!(line["listening"], format!("http://127.0.0.1:{port}"));
    assert_eq!(line["agent"], "Wren");
    assert_eq!(line["world"], "Moor");
    let dir = home.join("wren");
    let prompt = PathBuf::from(line["prompt"].as_str().unwrap());
    assert_eq!(prompt, dir.join("prompts").join("start.md"));
    assert!(prompt.is_file(), "{}", prompt.display());
    // The workspace names this session's URL.
    let agents = std::fs::read_to_string(dir.join("AGENTS.md")).unwrap();
    assert!(
        agents.contains(&format!("http://127.0.0.1:{port}")),
        "{agents}"
    );
    // run.json: this process, detached, on this port.
    let run = read_json(&dir.join("run.json"));
    assert_eq!(run["pid"], s.child.id(), "{run}");
    assert_eq!(run["port"], port, "{run}");
    assert_eq!(run["mode"], "detached", "{run}");
    assert_eq!(run["agent"], "Wren");
    assert_eq!(run["world"], "Moor");
    // The agent methods work.
    let unit = s.unit();
    let obs = s.call(
        "agent/observe",
        json!({"unit": unit, "controller": "explorer", "format": "text"}),
    );
    assert!(obs["text"].as_str().is_some_and(|t| !t.is_empty()), "{obs}");
    // A second host for the same agent is refused while this one runs.
    let out = npcctl()
        .args(serve_home(&home))
        .args(["--agent", "Wren", "--resume", "--port", "0"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("already running"), "{stdout}");
    let (code, _) = s.stop();
    assert_eq!(code, Some(0));
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn serve_saves_and_cleans_up_on_stop() {
    let home = home("stop");
    let s = serve(&home, &["--world", "Moor", "--paused"]);
    let dir = home.join("wren");
    assert!(dir.join("run.json").is_file());
    let step = s.call("npc/step", json!({"ticks": 25}));
    assert_eq!(step["tick"], 25, "{step}");
    let (code, last) = s.stop();
    assert_eq!(code, Some(0));
    assert_eq!(last["stopped"], true, "{last}");
    assert_eq!(last["tick"], 25, "{last}");
    assert!(!dir.join("run.json").exists(), "run.json removed");
    let snap = snapshot::read(&dir.join("moor.world.json")).unwrap();
    assert_eq!(snap.tick, 25, "saved on stop");
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn serve_resumes_the_same_tick() {
    let home = home("resume");
    let s = serve(&home, &["--world", "Moor", "--paused"]);
    let unit = s.unit();
    // Some state worth keeping: an action queued, ticks run.
    let act = s.call(
        "agent/act",
        json!({"unit": unit, "controller": "explorer", "steps": [
            {"verb": "order", "args": {"order": {"type": "Move", "pos": [2000, 1000]}}},
            {"verb": "execute_orders"}
        ]}),
    );
    assert_eq!(act["accepted"], true, "{act}");
    let before = s.call("npc/step", json!({"ticks": 37}));
    let (code, _) = s.stop();
    assert_eq!(code, Some(0));

    let s = serve(&home, &["--resume", "--paused"]);
    assert_eq!(s.line["world"], "Moor");
    assert!(
        s.line["prompt"]
            .as_str()
            .unwrap()
            .replace('\\', "/")
            .ends_with("prompts/resume.md"),
        "{}",
        s.line
    );
    let after = s.call("npc/step", json!({"ticks": 0}));
    assert_eq!(after["tick"], before["tick"], "{after} vs {before}");
    assert_eq!(after["hash"], before["hash"], "the same world");
    assert_eq!(s.unit(), unit);
    let (code, _) = s.stop();
    assert_eq!(code, Some(0));
    let _ = std::fs::remove_dir_all(&home);
}

/// Adds a diary entry as the agent: the note.
fn add_note(s: &Served, unit: u64, title: &str) -> Value {
    let added = s.call(
        "agent/notes/add",
        json!({"unit": unit, "controller": "explorer", "book": "diary",
               "title": title, "body": "Seen from the hill."}),
    );
    assert_eq!(added["accepted"], true, "{added}");
    added["note"].clone()
}

fn diary_titles(s: &Served, unit: u64) -> Vec<String> {
    let list = s.call(
        "agent/notes/list",
        json!({"unit": unit, "controller": "explorer", "book": "diary"}),
    );
    list["notes"]
        .as_array()
        .unwrap_or_else(|| panic!("{list}"))
        .iter()
        .map(|n| n["title"].as_str().unwrap().to_owned())
        .collect()
}

/// Every diary file's text.
fn diary_files(dir: &Path) -> Vec<String> {
    let mut out: Vec<String> = std::fs::read_dir(dir.join("diary"))
        .unwrap()
        .map(|e| {
            let mut s = String::new();
            std::fs::File::open(e.unwrap().path())
                .unwrap()
                .read_to_string(&mut s)
                .unwrap();
            s
        })
        .collect();
    out.sort();
    out
}

#[test]
fn notes_work_detached() {
    let home = home("notes");
    let s = serve(&home, &["--world", "Moor"]);
    let unit = s.unit();
    add_note(&s, unit, "The old mill");
    assert_eq!(diary_titles(&s, unit), ["The old mill"]);
    let files = diary_files(&home.join("wren"));
    assert_eq!(files.len(), 1, "{files:?}");
    assert!(files[0].contains("title: The old mill"), "{files:?}");
    assert!(
        files[0].contains("\nagent: Wren\nworld: Moor\n"),
        "{files:?}"
    );
    // The player's methods reach the same folder.
    let list = s.call("notes/list", json!({"unit": unit, "book": "diary"}));
    assert_eq!(list["notes"].as_array().unwrap().len(), 1, "{list}");
    let (code, _) = s.stop();
    assert_eq!(code, Some(0));
    let _ = std::fs::remove_dir_all(&home);
}

/// Ruling R6: `world/reset` rebuilds the explorer world with its notes
/// resources, so the notes still work afterwards; `world/load_scenario`
/// is refused (the explorer builds its own world).
#[test]
fn notes_survive_a_world_reset() {
    let home = home("reset");
    let s = serve(&home, &["--world", "Moor", "--paused"]);
    let unit = s.unit();
    add_note(&s, unit, "Before the reset");
    s.call("npc/step", json!({"ticks": 10}));
    let info = s.call("world/reset", json!({"seed": 99}));
    assert_eq!(info["tick"], 0, "{info}");
    assert_eq!(info["seed"], 99, "{info}");
    assert_eq!(s.unit(), unit, "the same actor");
    assert_eq!(diary_titles(&s, unit), ["Before the reset"]);
    add_note(&s, unit, "After the reset");
    assert_eq!(
        diary_titles(&s, unit),
        ["Before the reset", "After the reset"]
    );
    assert_eq!(diary_files(&home.join("wren")).len(), 2);
    // Fix round 1: the old world was kept, as --fresh-world keeps it, and
    // the save is the reset world.
    let dir = home.join("wren");
    let backups: Vec<String> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with("moor.world.") && n != "moor.world.json")
        .collect();
    assert_eq!(backups.len(), 1, "{backups:?}");
    let old = snapshot::read(&dir.join(&backups[0])).unwrap();
    assert_ne!(old.header["seed"], 99, "the backup is the old world");
    let new = snapshot::read(&dir.join("moor.world.json")).unwrap();
    assert_eq!((new.tick, new.header["seed"].as_u64()), (0, Some(99)));
    let err = s
        .client
        .call("world/load_scenario", json!({"scenario": {}}))
        .expect_err("refused");
    let ClientError::Rpc(e) = err else {
        panic!("{err}")
    };
    assert!(e.message.contains("explorer"), "{}", e.message);
    let (code, _) = s.stop();
    assert_eq!(code, Some(0));
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn serve_autosaves_on_its_interval() {
    let home = home("autosave");
    let s = serve(&home, &["--world", "Moor", "--autosave-secs", "1"]);
    let save = home.join("wren").join("moor.world.json");
    let deadline = Instant::now() + Duration::from_secs(120);
    loop {
        if snapshot::read(&save).is_ok_and(|snap| snap.tick > 0) {
            break;
        }
        assert!(Instant::now() < deadline, "no autosave past tick 0");
        std::thread::sleep(Duration::from_millis(200));
    }
    let (code, _) = s.stop();
    assert_eq!(code, Some(0));
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn explorer_flags_are_checked() {
    let home = home("flags");
    let h = home.to_str().unwrap();
    let actors = home.join("actors");
    // An invalid preset (a rover whose LIDAR has no beams; the built-in
    // rover is valid since Task 12).
    std::fs::create_dir_all(&actors).unwrap();
    std::fs::write(
        actors.join("broken.json"),
        testbed_content::actors::BUILTIN_PRESETS[1]
            .1
            .replace("\"beams\": 360", "\"beams\": 0"),
    )
    .unwrap();
    let actors = actors.to_str().unwrap();
    for (args, needle) in [
        (vec!["--explorer-home", h, "--world", "W"], "--agent"),
        (vec!["--explorer-home", h, "--agent", "A"], "--world"),
        (
            vec!["--explorer-home", h, "--agent", "A", "--scenario", "chop"],
            "--scenario",
        ),
        (
            vec![
                "--explorer-home",
                h,
                "--agent",
                "A",
                "--world",
                "W",
                "--preset",
                "broken",
                "--actors-dir",
                actors,
            ],
            // Task 11: an invalid preset is refused before anything starts.
            "actor preset `broken` is invalid",
        ),
        (
            vec![
                "--explorer-home",
                h,
                "--agent",
                "A",
                "--world",
                "W",
                "--token",
                "t",
            ],
            "--token",
        ),
        (
            vec![
                "--explorer-home",
                h,
                "--agent",
                "A",
                "--resume",
                "--world",
                "W",
            ],
            "--world",
        ),
        (
            vec!["--explorer-home", h, "--agent", "A", "--load-anyway"],
            "--resume",
        ),
        (vec![], "--scenario"),
    ] {
        let out = npcctl().arg("serve").args(&args).output().unwrap();
        assert_eq!(out.status.code(), Some(2), "{args:?}");
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(text.contains(needle), "{args:?}: {text}");
    }
    // Nothing was created for the refused New.
    assert!(!home.join("a").exists());
    let _ = std::fs::remove_dir_all(&home);
}

/// Fix round 1: a save from another build exits 3 (as `actor_testbed`),
/// with `needs_choice`; nothing starts.
#[test]
fn a_save_from_another_build_exits_3() {
    let home = home("mismatch");
    let s = serve(&home, &["--world", "Moor", "--paused"]);
    let (code, _) = s.stop();
    assert_eq!(code, Some(0));
    let save = home.join("wren").join("moor.world.json");
    let mut v = read_json(&save);
    v["content_hash"] = 12345.into();
    std::fs::write(&save, serde_json::to_vec(&v).unwrap()).unwrap();
    let out = npcctl()
        .args(serve_home(&home))
        .args(["--agent", "Wren", "--resume", "--port", "0"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(out.status.code(), Some(3), "{stdout}");
    let body: Value = serde_json::from_str(stdout.trim()).unwrap();
    assert_eq!(body["needs_choice"], true, "{body}");
    assert!(
        body["error"].as_str().unwrap().contains("--load-anyway"),
        "{body}"
    );
    assert!(!home.join("wren").join("run.json").exists());
    let _ = std::fs::remove_dir_all(&home);
}

/// Fix round 1: a taken `--port N` is found before anything is written: no
/// agent folder is left behind.
#[test]
fn a_taken_port_leaves_no_agent_folder() {
    let home = home("port-taken");
    let held = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = held.local_addr().unwrap().port().to_string();
    let out = npcctl()
        .args(serve_home(&home))
        .args(["--agent", "Wren", "--world", "Moor", "--port", &port])
        .output()
        .unwrap();
    drop(held);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(out.status.code(), Some(2), "{stdout}");
    assert!(
        stdout.contains(&format!("cannot listen on 127.0.0.1:{port}")),
        "{stdout}"
    );
    assert!(!home.join("wren").exists(), "no folder left behind");
    // Only the actor presets folder (`serve_home`, filled with the
    // built-ins when the preset was read).
    let left: Vec<_> = std::fs::read_dir(&home)
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .filter(|n| n != "actors")
        .collect();
    assert!(left.is_empty(), "{left:?}");
    let _ = std::fs::remove_dir_all(&home);
}

/// Task 8: `--wait-port` is a handover onto exactly `--port N`: it waits
/// (here ~1.5 s) for a port the closing window still holds, then resumes the
/// world on it. Without `--resume` and `--port` it is refused.
#[test]
fn wait_port_takes_the_port_once_it_is_released() {
    let home = home("wait-port");
    let s = serve(&home, &["--world", "Moor", "--paused"]);
    let (code, _) = s.stop();
    assert_eq!(code, Some(0));
    let held = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = held.local_addr().unwrap().port();
    let release = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(1500));
        drop(held);
    });
    let mut child = npcctl()
        .args(serve_home(&home))
        .args(["--agent", "Wren", "--resume", "--paused", "--wait-port"])
        .args(["--port", &port.to_string()])
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap();
    let lines = lines(child.stdout.take().unwrap());
    let first = lines.recv_timeout(START).unwrap_or_default();
    release.join().unwrap();
    let line: Value = serde_json::from_str(&first).unwrap_or(Value::Null);
    let client = RpcClient::new(&format!("http://127.0.0.1:{port}")).unwrap();
    let stopped = client.call("session/stop", json!({}));
    let deadline = Instant::now() + EXIT;
    while child.try_wait().unwrap().is_none() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(50));
    }
    let _ = child.kill();
    let _ = child.wait();
    assert_eq!(line["port"], port, "{first}");
    assert!(stopped.is_ok(), "{stopped:?}");

    let h = home.to_str().unwrap();
    for args in [
        vec![
            "--explorer-home",
            h,
            "--agent",
            "Wren",
            "--wait-port",
            "--port",
            "1",
        ],
        vec![
            "--explorer-home",
            h,
            "--agent",
            "Wren",
            "--resume",
            "--wait-port",
        ],
    ] {
        let out = npcctl().arg("serve").args(&args).output().unwrap();
        assert_eq!(out.status.code(), Some(2), "{args:?}");
        let text = String::from_utf8_lossy(&out.stderr);
        assert!(
            text.contains("--wait-port") || text.contains("--resume"),
            "{text}"
        );
    }
    let _ = std::fs::remove_dir_all(&home);
}
