//! `actor_testbed --explorer-home .. --resume --exact-port N` (plan Task 7):
//! the window takes over a detached world's port, retrying for 10 s, and
//! never moves to another port. A port that stays taken exits 4 before any
//! window opens.

use std::net::TcpListener;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use testbed_content::explorer::host::{HostArgs, build_world};

#[test]
fn exact_port_retries_then_exits_4() {
    let home = std::env::temp_dir().join(format!("testbed-exact-port-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&home).unwrap();
    // An agent to resume, so only the port can stop the start.
    let args = HostArgs {
        home: home.clone(),
        agent: "Wren".into(),
        world: Some("Moor".into()),
        resume: false,
        on_mismatch: None,
        preset: None,
        actors_dir: Some(home.join("actors")),
    };
    let _ = build_world(&args, 7, "2026-10-25T12:00:00Z", |_| {}).expect("a new agent");
    // A free port picked now, held for the child's whole life (more than
    // the 10 s it retries).
    let held = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = held.local_addr().unwrap().port();
    let started = Instant::now();
    let mut child = Command::new(env!("CARGO_BIN_EXE_actor_testbed"))
        .args(["--explorer-home", home.to_str().unwrap()])
        .args([
            "--agent",
            "Wren",
            "--resume",
            "--exact-port",
            &port.to_string(),
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("actor_testbed starts");
    // Generous: a busy machine may take a while to even start the exe.
    let deadline = started + Duration::from_secs(180);
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if Instant::now() > deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("actor_testbed did not give up on the taken port");
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    let waited = started.elapsed();
    drop(held);
    let mut stderr = String::new();
    std::io::Read::read_to_string(child.stderr.as_mut().unwrap(), &mut stderr).unwrap();
    assert_eq!(status.code(), Some(4), "{stderr}");
    assert!(
        stderr.contains(&format!("port {port} is still in use")),
        "{stderr}"
    );
    assert!(waited >= Duration::from_secs(10), "retried {waited:?}");
    // It never moved to another port: no run.json, the prompt files untouched.
    let dir = home.join("wren");
    assert!(!dir.join("run.json").exists());
    assert!(!dir.join("prompts").join("resume.md").exists());
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn exact_port_is_checked() {
    for (args, needle) in [
        (
            &["--exact-port", "15702"][..],
            "--exact-port need --explorer-home",
        ),
        (
            &[
                "--explorer-home",
                "h",
                "--agent",
                "A",
                "--world",
                "W",
                "--exact-port",
                "15702",
            ][..],
            "--resume",
        ),
        (
            &[
                "--explorer-home",
                "h",
                "--agent",
                "A",
                "--resume",
                "--exact-port",
                "x",
            ][..],
            "not a port",
        ),
    ] {
        let out = Command::new(env!("CARGO_BIN_EXE_actor_testbed"))
            .args(args)
            .output()
            .unwrap();
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert_eq!(out.status.code(), Some(2), "{args:?}: {stderr}");
        assert!(stderr.contains(needle), "{args:?}: {stderr}");
    }
}

/// Fix round 1: an agent whose world runs in another live process (its
/// `run.json`) is refused before any window opens: one owner per world.
#[test]
fn resume_refuses_an_agent_that_is_running() {
    use testbed_content::explorer::home::AgentDir;
    use testbed_content::explorer::host::{RunFile, RunMode};
    let home = std::env::temp_dir().join(format!("testbed-running-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&home).unwrap();
    let args = HostArgs {
        home: home.clone(),
        agent: "Wren".into(),
        world: Some("Moor".into()),
        resume: false,
        on_mismatch: None,
        preset: None,
        actors_dir: Some(home.join("actors")),
    };
    let (_, _, dir) = build_world(&args, 7, "2026-10-25T12:00:00Z", |_| {}).expect("a new agent");
    // A child this test owns stands in for the detached host.
    let mut sleeper = if cfg!(windows) {
        Command::new("ping")
            .args(["-n", "120", "127.0.0.1"])
            .stdout(Stdio::null())
            .spawn()
    } else {
        Command::new("sleep").arg("120").spawn()
    }
    .expect("a sleeping child");
    let run = RunFile {
        pid: sleeper.id(),
        port: 15799,
        mode: RunMode::Detached,
        started: "2026-10-25T12:00:00Z".into(),
        agent: "Wren".into(),
        world: "Moor".into(),
    };
    run.write(&dir).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_actor_testbed"))
        .args(["--explorer-home", home.to_str().unwrap()])
        .args(["--agent", "Wren", "--resume"])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("actor_testbed starts");
    let deadline = Instant::now() + Duration::from_secs(180);
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if Instant::now() > deadline {
            let _ = child.kill();
            let _ = child.wait();
            let _ = sleeper.kill();
            panic!("actor_testbed started an agent that is running elsewhere");
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    let _ = sleeper.kill();
    let _ = sleeper.wait();
    let mut stderr = String::new();
    std::io::Read::read_to_string(child.stderr.as_mut().unwrap(), &mut stderr).unwrap();
    assert_eq!(status.code(), Some(2), "{stderr}");
    let want = format!(
        "agent Wren is already running (pid {}, mode detached, port 15799)",
        run.pid
    );
    assert!(stderr.contains(&want), "{stderr}");
    // Its run.json is left alone.
    let on_disk = std::fs::read_to_string(RunFile::path(&AgentDir {
        dir: dir.dir.clone(),
    }))
    .unwrap();
    assert!(on_disk.contains(&run.pid.to_string()), "{on_disk}");
    let _ = std::fs::remove_dir_all(&home);
}
