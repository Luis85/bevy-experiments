//! What the menu can start and how (pure: no Bevy, no processes).

use std::path::{Path, PathBuf};

use serde::Deserialize;
use testbed_content::explorer::home::{name_problem, slug};
/// Where an explorer world runs: in the game window or detached
/// (`npcctl serve`).
pub use testbed_content::explorer::host::RunMode;
use testbed_content::explorer::host::detached_resume_args;
use testbed_content::explorer::settings::{Settings, validate_agents_dir};
use testbed_content::scenario::embedded_scenarios;

/// The choice on resuming a world saved by another build (`actor_testbed`'s
/// `--load-anyway` / `--fresh-world`; re-declared here, the apps share no
/// crate).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mismatch {
    LoadAnyway,
    FreshWorld,
}

/// Something the menu can launch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Entry {
    Village,
    Testbed(&'static str),
    RtsDemo,
    ExplorerNew {
        agent: String,
        world: String,
        mode: RunMode,
    },
    ExplorerResume {
        agent: String,
        choice: Option<Mismatch>,
        mode: RunMode,
    },
    /// Open in window: the stopped detached world in the window on exactly
    /// its port (`--resume --exact-port`).
    ExplorerMove {
        agent: String,
        port: u16,
    },
    /// Back to detached on `port` after a move that failed (`npcctl serve
    /// --resume --port N --wait-port`).
    ExplorerRestart {
        agent: String,
        port: u16,
    },
}

impl Entry {
    /// The name the Running screen and errors use ("Actor Testbed: doors").
    pub fn title(&self) -> String {
        match self {
            Self::Village => "Village".into(),
            Self::Testbed(name) => format!("Actor Testbed: {name}"),
            Self::RtsDemo => "RTS units demo".into(),
            Self::ExplorerNew { agent, .. }
            | Self::ExplorerResume { agent, .. }
            | Self::ExplorerMove { agent, .. }
            | Self::ExplorerRestart { agent, .. } => {
                if self.is_detached() {
                    format!("AI explorer: {agent} (detached)")
                } else {
                    format!("AI explorer: {agent}")
                }
            }
        }
    }

    /// Runs detached (`npcctl serve`): the menu stays and tracks it.
    pub fn is_detached(&self) -> bool {
        matches!(
            self,
            Self::ExplorerNew {
                mode: RunMode::Detached,
                ..
            } | Self::ExplorerResume {
                mode: RunMode::Detached,
                ..
            } | Self::ExplorerRestart { .. }
        )
    }

    /// The explorer agent this entry starts, if any.
    pub fn agent(&self) -> Option<&str> {
        match self {
            Self::ExplorerNew { agent, .. }
            | Self::ExplorerResume { agent, .. }
            | Self::ExplorerMove { agent, .. }
            | Self::ExplorerRestart { agent, .. } => Some(agent),
            _ => None,
        }
    }
}

/// The executable stem and arguments that start `entry`. Explorer entries
/// use the settings' agents folder as `--explorer-home` and, when the
/// settings name one, their actor presets folder as `--actors-dir` (else the
/// hosts' default beside the agents folder); those in the window
/// (`actor_testbed`) pass the fog of war setting as `--fog on|off`.
pub fn command_for(entry: &Entry, settings: &Settings) -> (&'static str, Vec<String>) {
    let home = || {
        let mut args = vec![
            "--explorer-home".to_owned(),
            settings.agents_dir.display().to_string(),
        ];
        args.extend(actors_args(settings));
        args
    };
    match entry {
        Entry::Village => ("village", Vec::new()),
        Entry::RtsDemo => ("rts_units_demo", Vec::new()),
        Entry::Testbed(name) => (
            "actor_testbed",
            vec!["--scenario".to_owned(), (*name).to_owned()],
        ),
        Entry::ExplorerNew { agent, world, mode } => {
            let mut args = home();
            args.extend([
                "--agent".into(),
                agent.clone(),
                "--world".into(),
                world.clone(),
            ]);
            with_mode(*mode, args, settings)
        }
        Entry::ExplorerResume {
            agent,
            choice,
            mode,
        } => {
            let mut args = home();
            args.extend(["--agent".into(), agent.clone(), "--resume".into()]);
            match choice {
                Some(Mismatch::LoadAnyway) => args.push("--load-anyway".into()),
                Some(Mismatch::FreshWorld) => args.push("--fresh-world".into()),
                None => {}
            }
            with_mode(*mode, args, settings)
        }
        Entry::ExplorerMove { agent, port } => {
            let mut args = home();
            args.extend([
                "--agent".into(),
                agent.clone(),
                "--resume".into(),
                "--exact-port".into(),
                port.to_string(),
            ]);
            args.extend(fog_args(settings));
            ("actor_testbed", args)
        }
        Entry::ExplorerRestart { agent, port } => {
            let mut args = detached_resume_args(&settings.agents_dir, agent, *port);
            args.extend(actors_args(settings));
            ("npcctl", args)
        }
    }
}

/// `--actors-dir DIR` when the settings name an actor presets folder.
pub fn actors_args(settings: &Settings) -> Vec<String> {
    settings
        .actors_dir
        .as_ref()
        .map(|d| vec!["--actors-dir".to_owned(), d.display().to_string()])
        .unwrap_or_default()
}

/// `actor_testbed ARGS --fog on|off` in the window, `npcctl serve ARGS`
/// detached (the same explorer flags; a detached world has no view).
fn with_mode(
    mode: RunMode,
    mut args: Vec<String>,
    settings: &Settings,
) -> (&'static str, Vec<String>) {
    match mode {
        RunMode::Window => {
            args.extend(fog_args(settings));
            ("actor_testbed", args)
        }
        RunMode::Detached => {
            let mut serve = vec!["serve".to_owned()];
            serve.extend(args);
            ("npcctl", serve)
        }
    }
}

/// `--fog on|off`: the settings' fog of war for an explorer window.
pub fn fog_args(settings: &Settings) -> [String; 2] {
    let on = if settings.fog_of_war { "on" } else { "off" };
    ["--fog".to_owned(), on.to_owned()]
}

/// The executable `stem` in `dir` (with the platform's suffix, `.exe` on
/// Windows), if it is a file.
pub fn find_exe(stem: &str, dir: &Path) -> Option<PathBuf> {
    let path = dir.join(format!("{stem}{}", std::env::consts::EXE_SUFFIX));
    path.is_file().then_some(path)
}

/// The command that builds `stem`.
pub fn build_command(stem: &str) -> Vec<String> {
    ["cargo", "build", "-p", stem].map(str::to_owned).to_vec()
}

/// `Ok` when the explorer can start, else why not (shown on Main).
///
/// # Errors
/// "set a valid agents folder in Settings" plus the validation error.
pub fn explorer_enabled(settings: &Settings) -> Result<(), String> {
    validate_agents_dir(&settings.agents_dir)
        .map_err(|e| format!("set a valid agents folder in Settings ({e})"))
}

/// [`build_command`] for the profile the menu itself runs from: `--release`
/// when `exe_dir` is a `release` target folder, so the build lands next to
/// the menu.
pub fn build_args(stem: &str, exe_dir: &Path) -> Vec<String> {
    let mut args = build_command(stem);
    if exe_dir.file_name().is_some_and(|n| n == "release") {
        args.push("--release".into());
    }
    args
}

/// The user's home folder from the environment (`USERPROFILE`, else
/// `HOME`); empty when neither is set.
pub fn home_dir(env: &dyn Fn(&str) -> Option<String>) -> PathBuf {
    ["USERPROFILE", "HOME"]
        .iter()
        .find_map(|k| env(k).filter(|v| !v.is_empty()))
        .map(PathBuf::from)
        .unwrap_or_default()
}

/// The Settings screen's Save: validates `agents_dir` (trimmed), checks
/// that there is a settings file to write (the platform has a config
/// folder), creates the folder when it is missing, then writes the settings
/// to `file`. A Save that fails before the write creates no folder.
///
/// # Errors
/// The validation, folder or file error, one line.
pub fn save_settings(
    agents_dir: &str,
    fog_of_war: bool,
    file: Option<&Path>,
) -> Result<Settings, String> {
    let dir = PathBuf::from(agents_dir.trim());
    validate_agents_dir(&dir)?;
    let file = file.ok_or("no settings folder: APPDATA, XDG_CONFIG_HOME and HOME are unset")?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    // The actors folder is not on this screen: keep what the file has.
    let actors_dir = Settings::load(file, Path::new("")).actors_dir;
    let settings = Settings {
        agents_dir: dir,
        fog_of_war,
        actors_dir,
    };
    settings
        .save(file)
        .map_err(|e| format!("cannot write {}: {e}", file.display()))?;
    Ok(settings)
}

/// The embedded scenarios the Testbed screen lists: all but `explorer`,
/// which has its own Main entry (Ruling R4).
pub fn testbed_scenarios() -> impl Iterator<Item = (&'static str, &'static str)> {
    embedded_scenarios()
        .iter()
        .copied()
        .filter(|(name, _)| *name != "explorer")
}

/// `args` joined for display, quoting the ones with spaces.
pub fn command_line(args: &[String]) -> String {
    args.iter()
        .map(|a| {
            if a.is_empty() || a.contains(char::is_whitespace) {
                format!("\"{a}\"")
            } else {
                a.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// The workspace the menu builds in: the nearest folder at or above `start`
/// (the menu's own folder) holding a `Cargo.toml` with a `[workspace]`
/// table, else the workspace this menu was compiled in when it still exists
/// (a `CARGO_TARGET_DIR` outside the workspace).
pub fn workspace_root(start: &Path) -> Option<PathBuf> {
    workspace_root_or(start, Path::new(env!("CARGO_MANIFEST_DIR")))
}

/// [`workspace_root`] with `fallback` (a folder inside the compile-time
/// workspace) as the second try.
pub fn workspace_root_or(start: &Path, fallback: &Path) -> Option<PathBuf> {
    workspace_above(start).or_else(|| workspace_above(fallback))
}

/// The nearest folder at or above `start` holding a `Cargo.toml` with a
/// `[workspace]` table.
fn workspace_above(start: &Path) -> Option<PathBuf> {
    start.ancestors().find_map(|dir| {
        let text = std::fs::read_to_string(dir.join("Cargo.toml")).ok()?;
        text.lines()
            .any(|l| l.trim() == "[workspace]")
            .then(|| dir.to_path_buf())
    })
}

/// `program` (plus the platform's executable suffix) in one of the
/// folders of `path_var` (a `PATH` value).
pub fn find_on_path(program: &str, path_var: &std::ffi::OsStr) -> Option<PathBuf> {
    std::env::split_paths(path_var).find_map(|dir| find_exe(program, &dir))
}

/// The one-line error Main shows after a game exited, or `None` when it
/// exited 0: the code, then the last line the game wrote to stderr (`why`).
pub fn exit_error(title: &str, code: Option<i32>, why: Option<&str>) -> Option<String> {
    let head = match code {
        Some(0) => return None,
        Some(c) => format!("{title} exited with code {c}"),
        None => format!("{title} was stopped (no exit code)"),
    };
    Some(with_reason(head, why))
}

/// `head: why`, or `head` alone without a reason.
pub fn with_reason(head: String, why: Option<&str>) -> String {
    match why {
        Some(why) if !why.is_empty() => format!("{head}: {why}"),
        _ => head,
    }
}

/// Why a `--check-save` that exited neither 0 nor 1 failed (the Resume
/// screen shows it instead of the mismatch dialog).
pub fn check_failed(code: Option<i32>, why: Option<&str>) -> String {
    let head = code.map_or_else(
        || "the save check failed (no exit code)".to_owned(),
        |c| format!("the save check failed (exit {c})"),
    );
    with_reason(head, why)
}

/// The longest stderr line kept as a reason (in characters).
const REASON_CHARS: usize = 240;

/// A child's stderr line as an error reason: ANSI escapes (Bevy's coloured
/// log) and control characters removed, trimmed, at most [`REASON_CHARS`];
/// `None` for a blank line and for the panic hook's `RUST_BACKTRACE` note,
/// which follows the panic message and says nothing about it.
pub fn reason_line(raw: &str) -> Option<String> {
    let mut out = String::with_capacity(raw.len());
    let mut chars = raw.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            // CSI: ESC [ parameters, then a final byte in 0x40..=0x7E.
            if chars.peek() == Some(&'[') {
                chars.next();
                for f in chars.by_ref() {
                    if ('\u{40}'..='\u{7e}').contains(&f) {
                        break;
                    }
                }
            }
            continue;
        }
        if !c.is_control() || c == '\t' {
            out.push(c);
        }
    }
    let line = out.trim();
    if line.is_empty() || line.starts_with("note: run with `RUST_BACKTRACE") {
        return None;
    }
    Some(line.chars().take(REASON_CHARS).collect())
}

/// What `actor_testbed --check-save` said about a save.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CheckResult {
    /// It fits this build: resume.
    Ok,
    /// Saved by another build (format version or content hash): the error.
    Mismatch(String),
    /// Missing, truncated or not a save (or the check printed no result).
    Unreadable(String),
}

/// Reads the `--check-save` result from the child's stdout: the last line
/// that is a `{"ok":..}` object (log lines before it are skipped). `kind`
/// `content_hash` / `version` is a [`CheckResult::Mismatch`]; `unreadable`,
/// another kind, or no such line is [`CheckResult::Unreadable`].
pub fn parse_check(stdout: &str) -> CheckResult {
    #[derive(Deserialize)]
    struct Line {
        ok: bool,
        #[serde(default)]
        error: Option<String>,
        #[serde(default)]
        kind: Option<String>,
    }
    let line = stdout
        .lines()
        .rev()
        .map(str::trim)
        .filter(|l| l.starts_with('{'))
        .find_map(|l| serde_json::from_str::<Line>(l).ok());
    match line {
        Some(Line { ok: true, .. }) => CheckResult::Ok,
        Some(Line { error, kind, .. }) => {
            let error = error.unwrap_or_else(|| "the save does not fit this build".to_owned());
            match kind.as_deref() {
                Some("content_hash" | "version") => CheckResult::Mismatch(error),
                _ => CheckResult::Unreadable(error),
            }
        }
        None => {
            let out = stdout.trim();
            let shown: String = out.chars().take(200).collect();
            CheckResult::Unreadable(if out.is_empty() {
                "actor_testbed --check-save printed no result".to_owned()
            } else {
                format!("unexpected actor_testbed --check-save output: {shown}")
            })
        }
    }
}

/// Why **Start** is disabled on the New screen, or `None`: a name
/// [`name_problem`] rejects (blank, or no letters or digits a folder can
/// use), or an agent folder `root/<slug(agent)>` that exists already. A
/// missing `root` has no agents yet (`actor_testbed` creates it).
pub fn new_world_problem(root: &Path, agent: &str, world: &str) -> Option<String> {
    let (agent, world) = (agent.trim(), world.trim());
    if let Some(p) = name_problem(agent) {
        return Some(format!("Agent name: {p}"));
    }
    if let Some(p) = name_problem(world) {
        return Some(format!("World name: {p}"));
    }
    root.join(slug(agent))
        .exists()
        .then(|| "an agent with this name exists: resume it instead".to_owned())
}

/// The New screen's line on where the agent lives: the folder
/// `actor_testbed --explorer-home root --agent <agent>` prepares
/// (`root/<slug(agent)>`, with `AGENTS.md`, the prompts and the AI tool's
/// settings), or only the agents folder while the name has no folder name.
pub fn agent_folder_line(root: &Path, agent: &str) -> String {
    let agent = agent.trim();
    if name_problem(agent).is_some() {
        return format!("The agent's folder goes into {}", root.display());
    }
    format!(
        "After Start, open your AI tool in {} and paste the prompt.",
        root.join(slug(agent)).display()
    )
}

/// `2026-10-25T01:46:07Z` as `2026-10-25 01:46 UTC` (another offset drops
/// it; text that is not an RFC 3339 date comes back unchanged).
pub fn short_date(rfc3339: &str) -> String {
    let b = rfc3339.as_bytes();
    let shaped = b.len() >= 16
        && b[4] == b'-'
        && b[7] == b'-'
        && matches!(b[10], b'T' | b't' | b' ')
        && b[13] == b':';
    if !shaped || !rfc3339.is_char_boundary(16) {
        return rfc3339.to_owned();
    }
    let utc = if rfc3339.ends_with(['Z', 'z']) {
        " UTC"
    } else {
        ""
    };
    format!("{} {}{utc}", &rfc3339[..10], &rfc3339[11..16])
}

/// The `header.saved` date of the save at `path` (read with the rest of the
/// file skipped, not kept), or `None` when it cannot be read.
pub fn saved_date(path: &Path) -> Option<String> {
    #[derive(Deserialize)]
    struct Save {
        header: Header,
    }
    #[derive(Deserialize)]
    struct Header {
        saved: String,
    }
    let file = std::fs::File::open(path).ok()?;
    let save: Save = serde_json::from_reader(std::io::BufReader::new(file)).ok()?;
    Some(save.header.saved)
}

/// The `actor_testbed` arguments that check `agent`'s save (no window).
pub fn check_args(agent: &str, settings: &Settings) -> Vec<String> {
    vec![
        "--explorer-home".to_owned(),
        settings.agents_dir.display().to_string(),
        "--agent".to_owned(),
        agent.to_owned(),
        "--check-save".to_owned(),
    ]
}

/// The Resume list's second column: the world and its last save.
pub fn resume_detail(world: &str, saved: Option<&str>, has_save: bool) -> String {
    match (has_save, saved) {
        (false, _) => format!("{world}  -  no save yet"),
        (true, Some(date)) => format!("{world}  -  last saved {}", short_date(date)),
        (true, None) => format!("{world}  -  saved (date unknown)"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("game-menu-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn settings(dir: &Path) -> Settings {
        Settings {
            agents_dir: dir.to_path_buf(),
            fog_of_war: true,
            actors_dir: None,
        }
    }

    fn strings(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn command_for_testbed_scenario() {
        let s = settings(Path::new("/agents"));
        assert_eq!(
            command_for(&Entry::Testbed("doors"), &s),
            ("actor_testbed", strings(&["--scenario", "doors"]))
        );
        assert_eq!(command_for(&Entry::Village, &s), ("village", vec![]));
        assert_eq!(command_for(&Entry::RtsDemo, &s), ("rts_units_demo", vec![]));
    }

    #[test]
    fn command_for_explorer_new() {
        let home = std::env::temp_dir().join("my agents");
        let entry = Entry::ExplorerNew {
            agent: "Wren Ashdown".into(),
            world: "Gloomhollow".into(),
            mode: RunMode::Window,
        };
        let (stem, args) = command_for(&entry, &settings(&home));
        assert_eq!(stem, "actor_testbed");
        let home = home.display().to_string();
        assert_eq!(
            args,
            strings(&[
                "--explorer-home",
                &home,
                "--agent",
                "Wren Ashdown",
                "--world",
                "Gloomhollow",
                "--fog",
                "on"
            ])
        );
        // Fog off in the settings: `--fog off`.
        let mut off = settings(Path::new("/agents"));
        off.fog_of_war = false;
        let (_, args) = command_for(&entry, &off);
        assert_eq!(args[args.len() - 2..], strings(&["--fog", "off"]));
    }

    /// Task 11 fix round 1: a set actors folder reaches every explorer host,
    /// windowed and detached; an unset one adds nothing.
    #[test]
    fn command_for_passes_the_actors_folder() {
        let dir = Path::new("/agents");
        let mut s = settings(dir);
        let new = Entry::ExplorerNew {
            agent: "Wren".into(),
            world: "Moor".into(),
            mode: RunMode::Window,
        };
        assert!(!command_for(&new, &s).1.contains(&"--actors-dir".to_owned()));
        let acts = Path::new("/my/actors");
        s.actors_dir = Some(acts.to_path_buf());
        let want = strings(&["--actors-dir", &acts.display().to_string()]);
        let entries = [
            new,
            Entry::ExplorerNew {
                agent: "Wren".into(),
                world: "Moor".into(),
                mode: RunMode::Detached,
            },
            Entry::ExplorerResume {
                agent: "Wren".into(),
                choice: Some(Mismatch::FreshWorld),
                mode: RunMode::Detached,
            },
            Entry::ExplorerMove {
                agent: "Wren".into(),
                port: 15702,
            },
            Entry::ExplorerRestart {
                agent: "Wren".into(),
                port: 15702,
            },
        ];
        for entry in entries {
            let (_, args) = command_for(&entry, &s);
            assert!(
                args.windows(2).any(|w| w == want.as_slice()),
                "{entry:?}: {args:?}"
            );
        }
    }

    #[test]
    fn command_for_resume_with_choice() {
        let s = settings(Path::new("/agents"));
        let home = Path::new("/agents").display().to_string();
        let resume = |choice| Entry::ExplorerResume {
            agent: "Wren".into(),
            choice,
            mode: RunMode::Window,
        };
        let base = ["--explorer-home", &home, "--agent", "Wren", "--resume"];
        let fog = strings(&["--fog", "on"]);
        let mut plain = strings(&base);
        plain.extend(fog.clone());
        assert_eq!(command_for(&resume(None), &s), ("actor_testbed", plain));
        let mut anyway = strings(&base);
        anyway.push("--load-anyway".into());
        anyway.extend(fog.clone());
        assert_eq!(
            command_for(&resume(Some(Mismatch::LoadAnyway)), &s),
            ("actor_testbed", anyway)
        );
        let mut fresh = strings(&base);
        fresh.push("--fresh-world".into());
        fresh.extend(fog);
        assert_eq!(
            command_for(&resume(Some(Mismatch::FreshWorld)), &s),
            ("actor_testbed", fresh)
        );
    }

    /// Task 8: Start detached runs `npcctl serve` with the same names and
    /// choices; the moves between the modes pin the port.
    #[test]
    fn detached_entry_builds_serve_args() {
        let s = settings(Path::new("/agents"));
        let home = Path::new("/agents").display().to_string();
        let new = Entry::ExplorerNew {
            agent: "Wren Ashdown".into(),
            world: "Gloomhollow".into(),
            mode: RunMode::Detached,
        };
        assert_eq!(
            command_for(&new, &s),
            (
                "npcctl",
                strings(&[
                    "serve",
                    "--explorer-home",
                    &home,
                    "--agent",
                    "Wren Ashdown",
                    "--world",
                    "Gloomhollow"
                ])
            )
        );
        let resume = |choice| Entry::ExplorerResume {
            agent: "Wren".into(),
            choice,
            mode: RunMode::Detached,
        };
        let base = [
            "serve",
            "--explorer-home",
            &home,
            "--agent",
            "Wren",
            "--resume",
        ];
        assert_eq!(command_for(&resume(None), &s), ("npcctl", strings(&base)));
        let mut fresh = strings(&base);
        fresh.push("--fresh-world".into());
        assert_eq!(
            command_for(&resume(Some(Mismatch::FreshWorld)), &s),
            ("npcctl", fresh)
        );
        // Back to detached on the same port, waiting for the window to go.
        let restart = Entry::ExplorerRestart {
            agent: "Wren".into(),
            port: 15707,
        };
        let mut again = strings(&base);
        again.extend(strings(&["--port", "15707", "--wait-port"]));
        assert_eq!(command_for(&restart, &s), ("npcctl", again));
        // Into the window on exactly that port.
        let open = Entry::ExplorerMove {
            agent: "Wren".into(),
            port: 15707,
        };
        assert_eq!(
            command_for(&open, &s),
            (
                "actor_testbed",
                strings(&[
                    "--explorer-home",
                    &home,
                    "--agent",
                    "Wren",
                    "--resume",
                    "--exact-port",
                    "15707",
                    "--fog",
                    "on"
                ])
            )
        );
        assert!(new.is_detached() && restart.is_detached() && !open.is_detached());
        assert_eq!(new.title(), "AI explorer: Wren Ashdown (detached)");
    }

    #[test]
    fn find_exe_adds_windows_suffix() {
        let d = tmp("exe");
        assert_eq!(find_exe("village", &d), None);
        let file = d.join(format!("village{}", std::env::consts::EXE_SUFFIX));
        std::fs::write(&file, "").unwrap();
        assert_eq!(find_exe("village", &d), Some(file));
        if cfg!(windows) {
            // A file without `.exe` is not the game.
            std::fs::write(d.join("npcctl"), "").unwrap();
            assert_eq!(find_exe("npcctl", &d), None);
            assert!(find_exe("village", &d).unwrap().ends_with("village.exe"));
        }
        // A folder named like the game is not the game either.
        std::fs::create_dir_all(d.join(format!("rts_units_demo{}", std::env::consts::EXE_SUFFIX)))
            .unwrap();
        assert_eq!(find_exe("rts_units_demo", &d), None);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn build_command_is_cargo_build_p() {
        assert_eq!(
            build_command("actor_testbed"),
            strings(&["cargo", "build", "-p", "actor_testbed"])
        );
    }

    #[test]
    fn explorer_disabled_without_valid_folder() {
        let e = explorer_enabled(&settings(Path::new("relative/agents"))).unwrap_err();
        assert!(e.contains("set a valid agents folder in Settings"), "{e}");
        assert!(e.contains("absolute"), "the reason is kept: {e}");
        assert!(explorer_enabled(&settings(Path::new(""))).is_err());
        let d = tmp("valid");
        assert_eq!(explorer_enabled(&settings(&d.join("agents"))), Ok(()));
        let _ = std::fs::remove_dir_all(&d);
    }
    #[test]
    fn testbed_screen_omits_explorer() {
        let names: Vec<&str> = testbed_scenarios().map(|(n, _)| n).collect();
        assert!(!names.contains(&"explorer"));
        assert!(names.contains(&"testbed") && names.contains(&"doors"));
        assert_eq!(names.len(), embedded_scenarios().len() - 1);
    }

    #[test]
    fn command_line_quotes_spaces() {
        assert_eq!(
            command_line(&strings(&["cargo", "build", "-p", "village"])),
            "cargo build -p village"
        );
        assert_eq!(
            command_line(&strings(&["a", "my agents", ""])),
            "a \"my agents\" \"\""
        );
    }

    #[test]
    fn workspace_root_walks_up_to_the_workspace_table() {
        let d = tmp("ws");
        std::fs::write(
            d.join("Cargo.toml"),
            "[workspace]
members = []
",
        )
        .unwrap();
        let member = d.join("apps").join("x");
        std::fs::create_dir_all(member.join("target")).unwrap();
        std::fs::write(
            member.join("Cargo.toml"),
            "[package]
name = \"x\"
",
        )
        .unwrap();
        assert_eq!(workspace_root(&member.join("target")), Some(d.clone()));
        let _ = std::fs::remove_dir_all(&d);
        // This crate's own manifest folder is inside the real workspace.
        let here = Path::new(env!("CARGO_MANIFEST_DIR"));
        let real = here.parent().and_then(Path::parent);
        assert_eq!(workspace_root(here).as_deref(), real);
    }

    #[test]
    fn workspace_root_falls_back_to_the_compile_time_workspace() {
        // A menu in a target folder outside any workspace (`CARGO_TARGET_DIR`).
        let outside = tmp("ws-outside");
        let here = Path::new(env!("CARGO_MANIFEST_DIR"));
        let real = here.parent().and_then(Path::parent).map(Path::to_path_buf);
        if workspace_above(&outside).is_none() {
            assert_eq!(workspace_root_or(&outside, here), real);
            assert_eq!(workspace_root(&outside), real, "the default fallback");
            // A compile-time workspace that is gone: no fallback.
            assert_eq!(workspace_root_or(&outside, &outside.join("gone")), None);
        }
        // A workspace above the menu wins over the fallback.
        std::fs::write(outside.join("Cargo.toml"), "[workspace]\n").unwrap();
        assert_eq!(workspace_root_or(&outside, here), Some(outside.clone()));
        let _ = std::fs::remove_dir_all(&outside);
    }

    #[test]
    fn find_on_path_searches_each_folder() {
        let (a, b) = (tmp("path-a"), tmp("path-b"));
        let exe = b.join(format!("cargo{}", std::env::consts::EXE_SUFFIX));
        std::fs::write(&exe, "").unwrap();
        let path = std::env::join_paths([&a, &b]).unwrap();
        assert_eq!(find_on_path("cargo", &path), Some(exe));
        assert_eq!(find_on_path("no-such-tool", &path), None);
        let _ = std::fs::remove_dir_all(&a);
        let _ = std::fs::remove_dir_all(&b);
    }

    #[test]
    fn exit_error_only_for_failures() {
        assert_eq!(exit_error("Village", Some(0), Some("ignored")), None);
        assert_eq!(
            exit_error("Actor Testbed: doors", Some(2), None).as_deref(),
            Some("Actor Testbed: doors exited with code 2")
        );
        assert_eq!(
            exit_error("Village", Some(101), Some("boom")).as_deref(),
            Some("Village exited with code 101: boom")
        );
        assert!(
            exit_error("Village", None, None)
                .unwrap()
                .contains("stopped")
        );
    }

    #[test]
    fn check_failed_names_the_exit_and_the_reason() {
        assert_eq!(
            check_failed(Some(2), Some("usage: actor_testbed ...")),
            "the save check failed (exit 2): usage: actor_testbed ..."
        );
        assert_eq!(
            check_failed(Some(101), None),
            "the save check failed (exit 101)"
        );
        assert!(check_failed(None, Some("")).ends_with("(no exit code)"));
    }

    #[test]
    fn reason_line_strips_colour_and_skips_the_backtrace_note() {
        assert_eq!(
            reason_line("\u{1b}[2m2026\u{1b}[0m \u{1b}[31mERROR\u{1b}[0m bad save\r\n").as_deref(),
            Some("2026 ERROR bad save")
        );
        assert_eq!(reason_line("   \r\n"), None);
        assert_eq!(
            reason_line(
                "note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace"
            ),
            None
        );
        let long = "x".repeat(1000);
        assert_eq!(reason_line(&long).unwrap().len(), REASON_CHARS);
        assert_eq!(reason_line("a\u{7}b").as_deref(), Some("ab"));
    }

    #[test]
    fn entry_titles() {
        assert_eq!(Entry::Testbed("doors").title(), "Actor Testbed: doors");
        let resume = Entry::ExplorerResume {
            agent: "Wren".into(),
            choice: None,
            mode: RunMode::Window,
        };
        assert_eq!(resume.title(), "AI explorer: Wren");
    }
    #[test]
    fn build_args_match_the_menu_profile() {
        let debug = Path::new("/ws/target/debug");
        assert_eq!(build_args("village", debug), build_command("village"));
        let release = build_args("village", Path::new("/ws/target/release"));
        assert_eq!(release.last().map(String::as_str), Some("--release"));
    }

    #[test]
    fn home_dir_prefers_userprofile() {
        let env = |k: &str| match k {
            "USERPROFILE" => Some("C:/Users/me".to_owned()),
            "HOME" => Some("/home/me".to_owned()),
            _ => None,
        };
        assert_eq!(home_dir(&env), PathBuf::from("C:/Users/me"));
        let home_only = |k: &str| (k == "HOME").then(|| "/home/me".to_owned());
        assert_eq!(home_dir(&home_only), PathBuf::from("/home/me"));
        assert_eq!(home_dir(&|_| None), PathBuf::new());
    }

    #[test]
    fn save_settings_creates_the_folder_and_writes_the_file() {
        let d = tmp("save");
        let agents = d.join("deep").join("agents");
        let file = d.join("cfg").join("settings.json");
        let text = format!("  {}  ", agents.display());
        let saved = save_settings(&text, false, Some(&file)).unwrap();
        assert_eq!(saved.agents_dir, agents);
        assert!(!saved.fog_of_war, "the fog setting is saved too");
        assert!(agents.is_dir(), "created on Save");
        assert_eq!(Settings::load(&file, &d), saved);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn save_settings_rejects_without_touching_disk() {
        let d = tmp("save-bad");
        let file = d.join("settings.json");
        let e = save_settings("relative/agents", true, Some(&file)).unwrap_err();
        assert!(e.contains("absolute"), "{e}");
        assert!(!file.exists());
        assert!(save_settings("", true, Some(&file)).is_err());
        // No settings file: refused before the folder is created.
        let e = save_settings(&d.join("a").display().to_string(), true, None).unwrap_err();
        assert!(e.contains("no settings folder"), "{e}");
        assert!(!d.join("a").exists(), "no folder for a Save that fails");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn parse_check_ok() {
        assert_eq!(parse_check("{\"ok\":true}\n"), CheckResult::Ok);
        // Log lines before the result are skipped.
        assert_eq!(
            parse_check("INFO starting\r\n{\"ok\":true}\r\n"),
            CheckResult::Ok
        );
    }

    #[test]
    fn parse_check_mismatch() {
        let hash =
            r#"{"ok":false,"error":"the content hash differs: 1 vs 2","kind":"content_hash"}"#;
        assert_eq!(
            parse_check(hash),
            CheckResult::Mismatch("the content hash differs: 1 vs 2".into())
        );
        let version =
            r#"{"ok":false,"error":"format version 9, this build reads 1","kind":"version"}"#;
        assert_eq!(
            parse_check(version),
            CheckResult::Mismatch("format version 9, this build reads 1".into())
        );
        let bad = r#"{"ok":false,"error":"unreadable save x: EOF","kind":"unreadable"}"#;
        assert_eq!(
            parse_check(bad),
            CheckResult::Unreadable("unreadable save x: EOF".into())
        );
    }

    #[test]
    fn parse_check_garbage_is_unreadable() {
        for out in [
            "",
            "   \n",
            "not json",
            "{\"ok\":",
            "{\"status\":\"fine\"}",
            "[1,2]",
        ] {
            let CheckResult::Unreadable(e) = parse_check(out) else {
                panic!("`{out}` must be unreadable");
            };
            assert!(e.contains("--check-save"), "{e}");
        }
        // An unknown kind is not a mismatch either.
        let odd = r#"{"ok":false,"error":"boom","kind":"other"}"#;
        assert_eq!(parse_check(odd), CheckResult::Unreadable("boom".into()));
    }

    #[test]
    fn start_blocked_for_existing_agent() {
        let d = tmp("new-world");
        std::fs::create_dir_all(d.join("wren-ashdown")).unwrap();
        let exists = "an agent with this name exists: resume it instead";
        assert_eq!(
            new_world_problem(&d, "Wren Ashdown", "Hollowmere").as_deref(),
            Some(exists)
        );
        // The same slug is the same folder.
        assert_eq!(
            new_world_problem(&d, "  wren ASHDOWN! ", "Hollowmere").as_deref(),
            Some(exists)
        );
        assert_eq!(new_world_problem(&d, "Brin Marlow", "Hollowmere"), None);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn start_blocked_for_unusable_names() {
        let d = tmp("new-names");
        // `home::name_problem`'s reasons, per field.
        assert_eq!(
            new_world_problem(&d, "   ", "Hollowmere").as_deref(),
            Some("Agent name: enter a name")
        );
        assert_eq!(
            new_world_problem(&d, "Brin Marlow", "").as_deref(),
            Some("World name: enter a name")
        );
        let no_letters = new_world_problem(&d, "!!! ???", "Hollowmere").unwrap();
        assert!(
            no_letters.starts_with("Agent name: ") && no_letters.contains("letters or digits"),
            "{no_letters}"
        );
        let no_letters = new_world_problem(&d, "Brin", "--- ~~~").unwrap();
        assert!(no_letters.starts_with("World name: "), "{no_letters}");
        // A folder that does not exist yet has no agents.
        assert_eq!(
            new_world_problem(&d.join("not-yet"), "Wren Ashdown", "Hollowmere"),
            None
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn short_date_drops_seconds() {
        assert_eq!(short_date("2026-10-25T01:46:07Z"), "2026-10-25 01:46 UTC");
        assert_eq!(short_date("2026-10-25T01:46:07+02:00"), "2026-10-25 01:46");
        assert_eq!(short_date("yesterday"), "yesterday");
    }

    #[test]
    fn saved_date_reads_the_header_only() {
        let d = tmp("saved");
        let save = d.join("w.world.json");
        std::fs::write(
            &save,
            r#"{"version":1,"content_hash":5,"tick":0,"header":{"agent":"A","saved":"2026-10-25T01:46:07Z","seed":3},"entities":[{"id":1,"components":{}}],"resources":{}}"#,
        )
        .unwrap();
        assert_eq!(saved_date(&save).as_deref(), Some("2026-10-25T01:46:07Z"));
        std::fs::write(&save, "{\"header\":").unwrap();
        assert_eq!(saved_date(&save), None);
        assert_eq!(saved_date(&d.join("none.json")), None);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn resume_detail_names_the_save() {
        assert_eq!(
            resume_detail("Hollowmere", Some("2026-10-25T01:46:07Z"), true),
            "Hollowmere  -  last saved 2026-10-25 01:46 UTC"
        );
        assert_eq!(
            resume_detail("Hollowmere", None, false),
            "Hollowmere  -  no save yet"
        );
        assert!(resume_detail("H", None, true).contains("unknown"));
    }

    #[test]
    fn check_args_ask_for_the_check_only() {
        let s = settings(Path::new("/agents"));
        let home = Path::new("/agents").display().to_string();
        assert_eq!(
            check_args("Wren Ashdown", &s),
            strings(&[
                "--explorer-home",
                &home,
                "--agent",
                "Wren Ashdown",
                "--check-save"
            ])
        );
    }
}
