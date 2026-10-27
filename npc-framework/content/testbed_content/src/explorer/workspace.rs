//! The prepared agent folder (spec G5, §7): everything an AI tool started in
//! the agent's folder needs, generated on every start and resume.
//!
//! Generated ([`GENERATED`], rewritten every time, hand edits are lost):
//! `AGENTS.md` (template `prompts/agents.md`), `CLAUDE.md` (`@AGENTS.md`),
//! `prompts/<kind>.md`, `.claude/settings.json` (MCP-only permissions),
//! `.claude/settings.local.json` (enables the `npc` server) and `.mcp.json`
//! (the game's MCP server, `npcctl mcp`). Owned files are never written
//! here: the notes `diary/`, `memory/`, `proposals/` (and `diary.md.bak`,
//! `memory.md.bak` of a migrated folder), `map/`, `agent.json`, the world
//! saves (`*.world.json`) and `run.json`.
//!
//! `AGENTS.md` and the prompt files start with a small YAML frontmatter
//! ([`generated_frontmatter`]: `type`, `agent`, `world`, `generated: true`)
//! so the folder reads well as an Obsidian vault; it holds no date, so an
//! unchanged folder regenerates byte for byte. `CLAUDE.md` gets none (the
//! Claude Code import must stay exactly `@AGENTS.md`), and the prompt text
//! the game shows and copies ([`render_prompt`]) never includes it. No
//! clock; I/O only in [`write_workspace`].

use std::path::Path;

use npc_framework::agent_io::notes::frontmatter::Frontmatter;
use serde_json::json;

use super::home::{AgentDir, AgentMeta};
use super::prompt::{PromptKind, PromptVars, fill, prompt_values, render_prompt};
use crate::actors::{ActorPreset, Body, SensorSpec};

const AGENTS: &str = include_str!("../../prompts/agents.md");

/// The files and folders [`write_workspace`] owns, relative to the agent
/// folder (`prompts` is the folder of the prompt files).
pub const GENERATED: [&str; 6] = [
    "AGENTS.md",
    "CLAUDE.md",
    "prompts",
    ".claude/settings.json",
    ".claude/settings.local.json",
    ".mcp.json",
];

/// Prompt files of older folders, replaced by `prompts/<kind>.md`; deleted
/// on the next write.
const OLD_PROMPTS: [&str; 2] = ["start_prompt.md", "resume_prompt.md"];

/// `CLAUDE.md`: Claude Code imports `AGENTS.md` through it.
pub const CLAUDE_MD: &str = "@AGENTS.md\n";

/// `.claude/settings.json`: only the game's MCP tools and reading files in
/// the folder are allowed, without asking; no shell, no file writes, no web.
pub const SETTINGS_JSON: &str = r#"{
  "permissions": {
    "defaultMode": "dontAsk",
    "allow": ["mcp__npc__*", "Read(./**)"],
    "deny": ["Bash", "PowerShell", "Edit", "Write", "NotebookEdit", "WebFetch", "WebSearch", "Monitor"],
    "blockReadsOutsideWorkingDirectories": true
  }
}
"#;

/// `.claude/settings.local.json`: turns on the `npc` server of `.mcp.json`
/// without a prompt.
pub const SETTINGS_LOCAL_JSON: &str = r#"{
  "enabledMcpjsonServers": ["npc"]
}
"#;

/// What the agent's actor is, for `AGENTS.md`: [`ActorSummary::from_preset`]
/// of its actor preset.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActorSummary {
    /// The preset's name.
    pub name: String,
    /// The body (`human`, `robot`, ..).
    pub body: String,
    /// One or two sentences on what the body can do.
    pub description: String,
    /// One line per sense.
    pub sensors: Vec<String>,
    /// The actions it may take (verb names, with a hint where needed).
    pub actions: Vec<String>,
}

/// The actions `AGENTS.md` names, in this order (after `walk` and
/// `interact`), when the actor may take them; the others (`study`, `teach`,
/// `drive_to`, `unload`) the agent finds among its affordances.
const LISTED_ACTIONS: [&str; 13] = [
    "pick_up",
    "drop",
    "give",
    "open",
    "close",
    "lock",
    "unlock",
    "equip",
    "unequip",
    "use_tool_on",
    "craft",
    "mount",
    "dismount",
];

/// `mm` as metres: whole metres plain (`15`), else one decimal (`12.5`).
fn metres(mm: i32) -> String {
    if mm % 1000 == 0 {
        format!("{}", mm / 1000)
    } else {
        format!("{}.{}", mm / 1000, (mm % 1000).abs() / 100)
    }
}

/// One `AGENTS.md` line per sensor.
fn sensor_line(s: &SensorSpec) -> String {
    match *s {
        SensorSpec::Vision {
            range_mm,
            half_fov_deg,
        } => format!(
            "sight: {} m, {half_fov_deg} degrees to each side of where you face; walls block it",
            metres(range_mm)
        ),
        SensorSpec::Hearing { radius_mm } => {
            format!("hearing: {} m all around", metres(radius_mm))
        }
        SensorSpec::Reach { radius_mm } => format!(
            "reach: {}.{} m all around, through nothing solid",
            radius_mm / 1000,
            radius_mm % 1000 / 100
        ),
        SensorSpec::Lidar {
            range_mm,
            fov_deg,
            beams,
            ..
        } => format!(
            "lidar: {} m, {fov_deg} degrees, {beams} beams; distances to what it hits, no names",
            metres(range_mm)
        ),
        SensorSpec::Camera {
            range_mm,
            half_fov_deg,
        } => format!(
            "camera: {} m, {half_fov_deg} degrees to each side of where you face; tells what things are",
            metres(range_mm)
        ),
        SensorSpec::Radar {
            range_mm,
            half_fov_deg,
            ..
        } => format!(
            "radar: {} m, {half_fov_deg} degrees to each side; strongest on things that move; walls block it",
            metres(range_mm)
        ),
    }
}

impl ActorSummary {
    /// The summary of actor preset `p`: its name and body, its description
    /// followed by its speed and load, a line per sense (a `reach` of 0 has
    /// none) and `walk`, `interact` and the listed actions it may take.
    pub fn from_preset(p: &ActorPreset) -> Self {
        use crate::layout::INVENTORY_SLOTS;
        // Speed is mm per tick at 20 ticks per second.
        let speed_cm_s = p.speed_mm_per_tick * 20 / 10;
        let moves = match p.body {
            Body::Human => "Walks",
            Body::Robot => "Rolls",
        };
        let description = format!(
            "{} {moves} about {}.{} m/s and carries up to {INVENTORY_SLOTS} things ({} kg).",
            p.description.trim(),
            speed_cm_s / 100,
            speed_cm_s % 100 / 10,
            p.carry_capacity / 1000
        );
        let sensors = p
            .sensors
            .iter()
            .filter(|s| !matches!(s, SensorSpec::Reach { radius_mm } if *radius_mm <= 0))
            .map(sensor_line)
            .collect();
        let actions = ["walk", "interact (walk to a thing and use it)"]
            .into_iter()
            .chain(
                LISTED_ACTIONS
                    .into_iter()
                    .filter(|a| p.actions.contains(*a)),
            )
            .map(String::from)
            .collect();
        Self {
            name: p.name.clone(),
            body: p.body.name().into(),
            description,
            sensors,
            actions,
        }
    }

    /// The built-in human scout ([`crate::actors::human_preset`]).
    pub fn human_scout() -> Self {
        Self::from_preset(&crate::actors::human_preset())
    }
}

/// Everything the folder's files are made from.
#[derive(Clone, Debug)]
pub struct WorkspaceVars {
    /// The prompt's values (agent, world, url, unit, npcctl, folder).
    pub prompt: PromptVars,
    /// The agent's actor.
    pub actor: ActorSummary,
    /// Credentials are rendered only into AGENTS.md, never the prompts.
    pub identity: AgentIdentity,
    /// Immutable selected world rules; no live entity/character details.
    pub world: crate::worlds::WorldPreset,
}

#[derive(Clone)]
pub struct AgentIdentity {
    pub agent_id: String,
    pub token: String,
}

impl From<&AgentMeta> for AgentIdentity {
    fn from(meta: &AgentMeta) -> Self {
        Self {
            agent_id: meta.agent_id.clone(),
            token: meta.token.clone(),
        }
    }
}

impl std::fmt::Debug for AgentIdentity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AgentIdentity")
            .field("agent_id", &self.agent_id)
            .field("token", &"<redacted>")
            .finish()
    }
}

/// `AGENTS.md`: the mission, MCP registration credentials and bootstrap,
/// notebooks and rules. Character details arrive through `enter_world`.
pub fn render_agents_md(v: &WorkspaceVars) -> String {
    let mut values = prompt_values(&v.prompt);
    values.extend([
        ("AGENT_ID", v.identity.agent_id.clone()),
        ("TOKEN", v.identity.token.clone()),
        ("WORLD_RULES", render_world_rules(&v.world)),
        ("MAP_GUIDE",if v.world.systems.map {"## Your map\n\nKeep places worth returning to with `map_add`: use `place: \"here\"`, a sensed or remembered `thing`, or an explicit `pos`. The landmark stays at that position. Read your landmarks with `map_list`; rename, move or annotate one with `map_update`, passing the exact updated stamp you read. Walk back using an `order` with `{\"type\":\"Move\",\"landmark\":N}` through the MCP action tools. Your observations show the five nearest landmarks. Keep descriptions useful to a later you.\n".into()}else{String::new()}),
        (
            "MEMORY_NOTE",
            if v.world.perception.forget_after_ticks == 1200 {
                "Your character remembers what it sensed for about a minute".into()
            } else {
                format!(
                    "Your character remembers what it sensed for about {} seconds",
                    v.world.perception.forget_after_ticks / 20
                )
            },
        ),
    ]);
    fill(AGENTS, &values)
}

pub fn render_world_rules(world: &crate::worlds::WorldPreset) -> String {
    let mut lines = vec![format!("{}: {}", world.name, world.description.trim())];
    let mut active = false;
    for (name, need, recovery) in [
        ("Hunger", &world.needs.hunger, "eat"),
        ("Thirst", &world.needs.thirst, "drink"),
        ("Energy", &world.needs.energy, "rest"),
        ("Battery", &world.needs.battery, "charge"),
    ] {
        if need.on {
            active = true;
            lines.push(format!("{name} worsens over time. At {}%, movement falls to {}% speed; at {}%, your character collapses or shuts down. Use `{recovery}` at a suitable source and recover below {}% before continuing.",need.low_milli/1000,need.low_speed_pct,need.out_milli/1000,need.low_milli/1000));
        }
    }
    if !active {
        lines.push("No survival needs are enabled.".into());
    }
    if world.day_night.on {
        lines.push(format!("Day and night cycle every {} seconds. At night, sight and camera range are {}% of their daytime range; LIDAR and radar are unchanged.",world.day_night.cycle_ticks/20,world.day_night.night_vision_pct));
    } else {
        lines.push("Darkness does not shorten your senses.".into());
    }
    let detection = match world.detection {
        crate::scenario::Detection::PerRoll => {
            "Some things need a successful perception check to notice."
        }
        crate::scenario::Detection::Automatic => {
            "Detection is automatic when your senses reach a thing."
        }
    };
    lines.push(detection.into());
    for (name, on) in [
        ("Crafting", world.systems.crafting),
        ("Vehicles", world.systems.vehicles),
        ("Locked doors and keys", world.systems.locked_doors),
        ("Skill progression", world.systems.progression),
    ] {
        lines.push(format!(
            "{name}: {}.",
            if on { "enabled" } else { "disabled" }
        ));
    }
    lines
        .into_iter()
        .map(|line| format!("- {line}"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Character briefing returned by enter_world, never embedded in the
/// outsider's AGENTS.md or mission prompt.
pub fn render_character(a: &ActorSummary) -> String {
    let sensors = if a.sensors.is_empty() {
        "- none\n".into()
    } else {
        a.sensors
            .iter()
            .map(|s| format!("- {s}\n"))
            .collect::<String>()
    };
    let actions = if a.actions.is_empty() {
        "none".into()
    } else {
        a.actions
            .iter()
            .map(|act| match act.split_once(' ') {
                Some((verb, hint)) => format!("`{verb}` {hint}"),
                None => format!("`{act}`"),
            })
            .collect::<Vec<_>>()
            .join(", ")
    };
    format!(
        "## Your character: {}\n\nBody: {}. {}\n\nSenses:\n\n{}\nActions: {}.\n",
        a.name, a.body, a.description, sensors, actions
    )
}

/// The frontmatter a generated markdown file starts with: `type` (`kind`:
/// `agent-guide`, `prompt`), `agent`, `world` and `generated: true`; no
/// date, so the same values give the same bytes.
pub fn generated_frontmatter(kind: &str, v: &PromptVars) -> String {
    let mut fm = Frontmatter::new("\n");
    fm.set_text("type", kind);
    fm.set_text("agent", &v.agent);
    fm.set_text("world", &v.world);
    fm.set_plain("generated", "true");
    fm.render()
}

/// The `AGENTS.md` file: [`generated_frontmatter`] (`agent-guide`), then
/// [`render_agents_md`].
pub fn agents_md_file(v: &WorkspaceVars) -> String {
    format!(
        "{}{}",
        generated_frontmatter("agent-guide", &v.prompt),
        render_agents_md(v)
    )
}

/// The `prompts/<kind>.md` file: [`generated_frontmatter`] (`prompt`), then
/// the prompt ([`render_prompt`], the text the game copies, which has no
/// frontmatter).
pub fn prompt_file(kind: PromptKind, v: &PromptVars) -> String {
    format!(
        "{}{}",
        generated_frontmatter("prompt", v),
        render_prompt(kind, v)
    )
}

/// `.mcp.json`: the `npc` stdio server, `npcctl mcp --url URL --controller
/// explorer`. The command keeps the platform's path (Windows backslashes,
/// JSON-escaped): Claude Code runs it directly, not through a shell.
pub fn render_mcp_json(v: &PromptVars) -> String {
    let doc = json!({
        "mcpServers": {
            "npc": {
                "type": "stdio",
                "command": v.npcctl.display().to_string(),
                "args": ["mcp", "--url", v.url, "--registered"],
            }
        }
    });
    // A `Value` always serializes.
    let mut text = serde_json::to_string_pretty(&doc).unwrap_or_default();
    text.push('\n');
    text
}

/// Writes `text` to `path` through a temp file next to it and a rename, so
/// a reader never sees half a file. The temp name carries the process id,
/// so two processes writing the same folder never share one; it is removed
/// when either step fails.
pub(crate) fn write_atomic(path: &Path, text: &str) -> std::io::Result<()> {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let tmp = path.with_file_name(format!(".{name}.{}.tmp", std::process::id()));
    std::fs::write(&tmp, text)
        .and_then(|()| std::fs::rename(&tmp, path))
        .inspect_err(|_| {
            let _ = std::fs::remove_file(&tmp);
        })
}

/// Writes every generated file of `dir` for a `kind` start (each atomically:
/// a temp file, then a rename), removes the old `start_prompt.md` /
/// `resume_prompt.md`, and never touches an owned file. The prompt of
/// `kind` is (re)written; other prompts of earlier starts stay (they name
/// no port).
///
/// # Errors
/// The first file that cannot be written.
pub fn write_workspace(
    dir: &AgentDir,
    _kind: PromptKind,
    vars: &WorkspaceVars,
) -> std::io::Result<()> {
    let root = &dir.dir;
    std::fs::create_dir_all(root.join("prompts"))?;
    std::fs::create_dir_all(root.join(".claude"))?;
    write_atomic(&root.join("AGENTS.md"), &agents_md_file(vars))?;
    write_atomic(&root.join("CLAUDE.md"), CLAUDE_MD)?;
    for kind in [
        PromptKind::Start,
        PromptKind::Resume,
        PromptKind::ResumeChanged,
        PromptKind::ResumeRebuilt,
    ] {
        write_atomic(&dir.prompt(kind), &prompt_file(kind, &vars.prompt))?;
    }
    let claude = root.join(".claude");
    write_atomic(&claude.join("settings.json"), SETTINGS_JSON)?;
    write_atomic(&claude.join("settings.local.json"), SETTINGS_LOCAL_JSON)?;
    write_atomic(&root.join(".mcp.json"), &render_mcp_json(&vars.prompt))?;
    for old in OLD_PROMPTS {
        match std::fs::remove_file(root.join(old)) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e),
            _ => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::explorer::home::create_agent;
    use serde_json::Value;
    use std::path::PathBuf;

    const KINDS: [PromptKind; 4] = [
        PromptKind::Start,
        PromptKind::Resume,
        PromptKind::ResumeChanged,
        PromptKind::ResumeRebuilt,
    ];

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("npc-ws-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn vars_in(dir: &Path, npcctl: &str, url: &str) -> WorkspaceVars {
        WorkspaceVars {
            prompt: PromptVars {
                agent: "Wren Ashdown".into(),
                world: "Hollowmere".into(),
                url: url.into(),
                unit: 33,
                npcctl: PathBuf::from(npcctl),
                dir: dir.to_path_buf(),
                npcctl_missing: false,
                map_enabled: true,
            },
            actor: ActorSummary::human_scout(),
            world: crate::worlds::plain_preset(),
            identity: AgentIdentity {
                agent_id: "wren-ashdown-0123abcd".into(),
                token: "1".repeat(64),
            },
        }
    }

    fn vars() -> WorkspaceVars {
        vars_in(
            Path::new(r"C:\agents\wren-ashdown"),
            r"C:\games\npcctl.exe",
            "http://127.0.0.1:15702",
        )
    }

    #[test]
    fn agents_md_describes_the_robot() {
        for (_, json) in crate::actors::BUILTIN_PRESETS
            .iter()
            .filter(|(id, _)| *id != "human")
        {
            let preset: ActorPreset = serde_json::from_str(json).unwrap();
            let mut v = vars();
            v.actor = ActorSummary::from_preset(&preset);
            let md = render_character(&v.actor);
            assert!(
                md.contains(&preset.name) && md.contains("robot") && md.contains("Rolls"),
                "{md}"
            );
            assert!(md.contains("camera: 15 m"), "{md}");
            assert!(
                !md.contains("- sight:") && !md.contains("- hearing:"),
                "{md}"
            );
            assert!(
                md.contains(if preset.model == "rover" {
                    "no names"
                } else {
                    "radar: 30 m"
                }),
                "{md}"
            );
        }
    }

    fn agent(tag: &str) -> (PathBuf, AgentDir) {
        let home = tmp(tag);
        let a = create_agent(&home, "Wren Ashdown", "Hollowmere", "2026-10-25").unwrap();
        (home, a)
    }

    fn read_json(path: &Path) -> Value {
        serde_json::from_str(&std::fs::read_to_string(path).unwrap())
            .unwrap_or_else(|e| panic!("{}: {e}", path.display()))
    }

    /// Every file under `dir` with its bytes, sorted by path.
    fn snapshot(dir: &Path) -> Vec<(PathBuf, Vec<u8>)> {
        let mut out = Vec::new();
        let mut stack = vec![dir.to_path_buf()];
        while let Some(d) = stack.pop() {
            for e in std::fs::read_dir(&d).unwrap() {
                let p = e.unwrap().path();
                if p.is_dir() {
                    stack.push(p);
                } else {
                    out.push((p.clone(), std::fs::read(&p).unwrap()));
                }
            }
        }
        out.sort();
        out
    }

    #[test]
    fn workspace_writes_every_generated_file() {
        let (home, a) = agent("all");
        for kind in KINDS {
            let v = vars_in(&a.dir, r"C:\games\npcctl.exe", "http://127.0.0.1:15702");
            write_workspace(&a, kind, &v).unwrap();
            for g in GENERATED {
                assert!(a.dir.join(g).exists(), "{kind:?}: {g} missing");
            }
            assert!(a.dir.join("prompts").is_dir());
            let prompt = std::fs::read_to_string(a.prompt(kind)).unwrap();
            assert_eq!(prompt, prompt_file(kind, &v.prompt), "{kind:?}");
            let agents = std::fs::read_to_string(a.dir.join("AGENTS.md")).unwrap();
            assert_eq!(agents, agents_md_file(&v));
        }
        assert_eq!(
            a.prompt(PromptKind::Start),
            a.dir.join("prompts").join("start.md")
        );
        assert_eq!(
            a.prompt(PromptKind::ResumeRebuilt),
            a.dir.join("prompts").join("resume_rebuilt.md")
        );
        // Atomic writes leave no temp files behind.
        for (p, _) in snapshot(&a.dir) {
            let name = p.file_name().unwrap().to_string_lossy().into_owned();
            assert!(!name.ends_with(".tmp"), "{}", p.display());
        }
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn settings_are_mcp_only() {
        let (home, a) = agent("settings");
        write_workspace(&a, PromptKind::Start, &vars_in(&a.dir, "npcctl", "u")).unwrap();
        let s = read_json(&a.dir.join(".claude").join("settings.json"));
        // Exactly the Global Constraints JSON, nothing more.
        let want: Value = serde_json::from_str(
            r#"{"permissions":{"defaultMode":"dontAsk","allow":["mcp__npc__*","Read(./**)"],"deny":["Bash","PowerShell","Edit","Write","NotebookEdit","WebFetch","WebSearch","Monitor"],"blockReadsOutsideWorkingDirectories":true}}"#,
        )
        .unwrap();
        assert_eq!(s, want);
        let local = read_json(&a.dir.join(".claude").join("settings.local.json"));
        let want_local: Value =
            serde_json::from_str(r#"{"enabledMcpjsonServers":["npc"]}"#).unwrap();
        assert_eq!(local, want_local);
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn mcp_json_is_valid_with_a_windows_path() {
        let (home, a) = agent("mcp");
        let exe = r"C:\Program Files\npc\npcctl.exe";
        write_workspace(
            &a,
            PromptKind::Resume,
            &vars_in(&a.dir, exe, "http://127.0.0.1:15710"),
        )
        .unwrap();
        let text = std::fs::read_to_string(a.dir.join(".mcp.json")).unwrap();
        assert!(
            text.contains(r#""C:\\Program Files\\npc\\npcctl.exe""#),
            "{text}"
        );
        let m = read_json(&a.dir.join(".mcp.json"));
        let npc = &m["mcpServers"]["npc"];
        assert_eq!(npc["type"], "stdio");
        assert_eq!(npc["command"], exe);
        assert_eq!(
            npc["args"],
            json!(["mcp", "--url", "http://127.0.0.1:15710", "--registered"])
        );
        assert_eq!(m["mcpServers"].as_object().unwrap().len(), 1);
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn claude_md_imports_agents_md() {
        let (home, a) = agent("claude-md");
        write_workspace(&a, PromptKind::Start, &vars_in(&a.dir, "npcctl", "u")).unwrap();
        let text = std::fs::read_to_string(a.dir.join("CLAUDE.md")).unwrap();
        assert_eq!(text, "@AGENTS.md\n");
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn claude_md_is_exactly_the_import() {
        let (home, a) = agent("claude-md-exact");
        for kind in KINDS {
            write_workspace(&a, kind, &vars_in(&a.dir, "npcctl", "u")).unwrap();
            // No frontmatter: the import line is the whole file.
            let bytes = std::fs::read(a.dir.join("CLAUDE.md")).unwrap();
            assert_eq!(bytes, b"@AGENTS.md\n", "{kind:?}");
        }
        assert_eq!(CLAUDE_MD, "@AGENTS.md\n");
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn generated_files_have_frontmatter_and_are_stable() {
        let (home, a) = agent("frontmatter");
        let v = vars_in(&a.dir, r"C:\games\npcctl.exe", "http://127.0.0.1:15702");
        write_workspace(&a, PromptKind::Start, &v).unwrap();
        write_workspace(&a, PromptKind::Resume, &v).unwrap();
        let head = |ty: &str| {
            format!(
                "---\ntype: {ty}\nagent: Wren Ashdown\nworld: Hollowmere\ngenerated: true\n---\n"
            )
        };
        let agents = std::fs::read_to_string(a.dir.join("AGENTS.md")).unwrap();
        assert!(agents.starts_with(&head("agent-guide")), "{agents}");
        assert_eq!(
            agents,
            format!("{}{}", head("agent-guide"), render_agents_md(&v))
        );
        for kind in [PromptKind::Start, PromptKind::Resume] {
            let p = std::fs::read_to_string(a.prompt(kind)).unwrap();
            assert_eq!(
                p,
                format!("{}{}", head("prompt"), render_prompt(kind, &v.prompt))
            );
        }
        // An unchanged folder regenerates byte for byte (no date in it).
        let before = snapshot(&a.dir);
        write_workspace(&a, PromptKind::Resume, &v).unwrap();
        write_workspace(&a, PromptKind::Start, &v).unwrap();
        assert_eq!(snapshot(&a.dir), before);
        // Names YAML would misread are quoted.
        let mut odd = v.clone();
        odd.prompt.agent = "Wren: the second".into();
        odd.prompt.world = "#7".into();
        let fm = generated_frontmatter("prompt", &odd.prompt);
        assert_eq!(
            fm,
            "---\ntype: prompt\nagent: \"Wren: the second\"\nworld: \"#7\"\ngenerated: true\n---\n"
        );
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn copied_prompt_has_no_yaml() {
        let (home, a) = agent("copied");
        for kind in KINDS {
            let v = vars_in(&a.dir, "npcctl", "u");
            write_workspace(&a, kind, &v).unwrap();
            // The text the modal shows and the clipboard gets ...
            let text = render_prompt(kind, &v.prompt);
            for s in ["---", "generated:", "type: prompt", "agent: Wren"] {
                assert!(!text.contains(s), "{kind:?}: {s:?} in {text}");
            }
            // ... is the file without its frontmatter.
            let file = std::fs::read_to_string(a.prompt(kind)).unwrap();
            assert!(file.starts_with("---\ntype: prompt\n"), "{file}");
            assert!(file.ends_with(&text), "{kind:?}");
            assert_eq!(
                file.len() - text.len(),
                generated_frontmatter("prompt", &v.prompt).len()
            );
        }
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn owned_files_are_never_touched() {
        let (home, a) = agent("owned");
        // Owned files with bytes generation must keep: a BOM, CRLF, stray text.
        std::fs::create_dir_all(a.dir.join("map")).unwrap();
        let owned: [(PathBuf, &[u8]); 10] = [
            (
                a.diary().join("2026-10-25 x.md"),
                b"\xef\xbb\xbf---\r\nid: 1\r\n---\r\nstray\r\n",
            ),
            (
                a.memory().join("The store.md"),
                b"---\nid: 1\n---\nLocked.\n",
            ),
            (
                a.proposals().join("Doors.md"),
                b"---\nid: 1\nstatus: liked\n---\n",
            ),
            (a.dir.join("map").join("Gate.md"), b"---\nid: 1\n---\n"),
            (
                a.dir.join("diary.md.bak"),
                b"\xef\xbb\xbf# Diary\r\n## tick 1 - x\r\nstray\r\n",
            ),
            (a.dir.join("memory.md.bak"), b"## The store\nLocked.\n"),
            (a.dir.join("diary.md"), b"## tick 2 - y\n"),
            (
                a.meta(),
                b"{\"agent\":\"Wren Ashdown\",\"world\":\"Hollowmere\",\"created\":\"x\"}",
            ),
            (a.save("Hollowmere"), b"{\"save\":1}"),
            (a.dir.join("run.json"), b"{\"port\":15702}"),
        ];
        for (p, bytes) in &owned {
            std::fs::write(p, bytes).unwrap();
        }
        let before: Vec<_> = snapshot(&a.dir);
        for kind in KINDS {
            let v = vars_in(&a.dir, r"C:\games\npcctl.exe", "http://127.0.0.1:15799");
            write_workspace(&a, kind, &v).unwrap();
        }
        for (p, bytes) in &owned {
            assert_eq!(&std::fs::read(p).unwrap(), bytes, "{}", p.display());
        }
        // Nothing outside the generated set was added, changed or removed.
        let generated = |p: &Path| {
            let rel = p
                .strip_prefix(&a.dir)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            GENERATED
                .iter()
                .any(|g| rel == *g || rel.starts_with(&format!("{g}/")))
        };
        let after: Vec<_> = snapshot(&a.dir)
            .into_iter()
            .filter(|(p, _)| !generated(p))
            .collect();
        let before: Vec<_> = before.into_iter().filter(|(p, _)| !generated(p)).collect();
        assert_eq!(before, after);
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn old_prompt_files_are_removed() {
        let (home, a) = agent("old");
        std::fs::write(a.dir.join("start_prompt.md"), "old start").unwrap();
        std::fs::write(a.dir.join("resume_prompt.md"), "old resume").unwrap();
        write_workspace(&a, PromptKind::Resume, &vars_in(&a.dir, "npcctl", "u")).unwrap();
        assert!(!a.dir.join("start_prompt.md").exists());
        assert!(!a.dir.join("resume_prompt.md").exists());
        assert!(a.prompt(PromptKind::Resume).is_file());
        // A second write (no old files left) is fine too.
        write_workspace(&a, PromptKind::Resume, &vars_in(&a.dir, "npcctl", "u")).unwrap();
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn a_new_port_rewrites_the_generated_files() {
        let (home, a) = agent("port");
        write_workspace(
            &a,
            PromptKind::Start,
            &vars_in(&a.dir, "npcctl", "http://127.0.0.1:15702"),
        )
        .unwrap();
        std::fs::write(a.dir.join("AGENTS.md"), "hand edit").unwrap();
        write_workspace(
            &a,
            PromptKind::Resume,
            &vars_in(&a.dir, "npcctl", "http://127.0.0.1:15711"),
        )
        .unwrap();
        let agents = std::fs::read_to_string(a.dir.join("AGENTS.md")).unwrap();
        assert!(
            !agents.contains("15711") && !agents.contains("15702"),
            "{agents}"
        );
        let mcp = std::fs::read_to_string(a.dir.join(".mcp.json")).unwrap();
        assert!(mcp.contains("15711") && !mcp.contains("15702"), "{mcp}");
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn agents_md_is_a_mission_not_a_manual() {
        let md = render_agents_md(&vars());
        for s in [
            "You are Wren Ashdown",
            "Hollowmere",
            "Epoch 1",
            "Survive. Learn. Explore. Map the world.",
            "Nobody will explain the world",
            "`register`",
            "`enter_world`",
            "`take_control`",
            "the `npc` MCP server",
            "Each MCP tool describes its own inputs",
            "trust this folder",
        ] {
            assert!(md.contains(s), "missing {s:?}");
        }
        // The agent learns the tools and mechanics itself: no walkthrough of
        // the loop, the tool fields or the world's geometry.
        for s in [
            "act_and_wait",
            "expect_tick",
            "since_seq",
            "until_idle",
            "execute_orders",
            "Walls within",
            "head for an opening",
            "millimetres",
            "ticks per second",
        ] {
            assert!(!md.contains(s), "still teaches {s:?}");
        }
        // Every notes tool it names exists.
        let tools = npc_framework::agent_io::mcp::tools();
        for name in ["notes_list", "notes_add", "notes_update"] {
            assert!(md.contains(&format!("`{name}`")), "{name}");
            assert!(
                tools.as_array().unwrap().iter().any(|t| t["name"] == name),
                "no tool {name}"
            );
        }
    }

    #[test]
    fn agents_md_mentions_memory_span() {
        let md = render_agents_md(&vars());
        assert!(
            md.contains("remembers what it sensed for about a minute"),
            "memory span"
        );
        assert!(md.contains("Keep lasting knowledge in your notebooks"));
    }

    #[test]
    fn agents_md_teaches_only_available_map_tools_and_respects_world_switch() {
        let mut v = vars();
        let on = render_agents_md(&v);
        for text in [
            "## Your map",
            "`map_list`",
            "`map_add`",
            "`map_update`",
            "sensed or remembered",
            "\"landmark\":N",
        ] {
            assert!(on.contains(text), "missing {text}");
        }
        assert!(!on.contains("npcctl") && !on.contains("http://") && !on.contains("controller"));
        v.world.systems.map = false;
        v.prompt.map_enabled = false;
        let off = render_agents_md(&v);
        for text in [
            "## Your map",
            "map_list",
            "map_add",
            "map_update",
            "landmark",
        ] {
            assert!(!off.contains(text), "disabled map: {text}");
        }
    }

    #[test]
    fn agents_md_says_observations_show_searched_ground() {
        let md = render_agents_md(&vars());
        assert!(
            md.contains("observations show how much nearby ground it has searched"),
            "searched ground"
        );
    }

    /// Task 11: the human preset's summary says what the hard-coded scout
    /// said before presets (the description now leads with the preset's own
    /// sentence, then speed and load).
    #[test]
    fn human_preset_summary_matches_the_scout() {
        let h = ActorSummary::human_scout();
        assert_eq!(h.name, "Human scout");
        assert_eq!(h.body, "human");
        assert_eq!(
            h.description,
            "A person on foot: opens doors and gates, uses tools and rides vehicles. \
             Walks about 1.4 m/s and carries up to 8 things (30 kg)."
        );
        assert_eq!(
            h.sensors,
            [
                "sight: 15 m, 60 degrees to each side of where you face; walls block it",
                "hearing: 10 m all around",
                "reach: 1.5 m all around, through nothing solid",
            ]
        );
        assert_eq!(
            h.actions,
            [
                "walk",
                "interact (walk to a thing and use it)",
                "pick_up",
                "drop",
                "give",
                "open",
                "close",
                "lock",
                "unlock",
                "equip",
                "unequip",
                "use_tool_on",
                "craft",
                "mount",
                "dismount",
            ]
        );
    }

    #[test]
    fn agents_md_describes_the_actor() {
        let mut v = vars();
        v.actor = ActorSummary {
            name: "Rover R1".into(),
            body: "six-wheeled robot".into(),
            description: "Rolls; cannot climb stairs.".into(),
            sensors: vec!["lidar: 30 m, 360 degrees".into(), "camera: 20 m".into()],
            actions: vec!["pick_up".into(), "drop".into()],
        };
        let md = render_character(&v.actor);
        for s in [
            "Rover R1",
            "six-wheeled robot",
            "Rolls; cannot climb stairs.",
            "- lidar: 30 m, 360 degrees\n- camera: 20 m\n",
            "`pick_up`, `drop`",
        ] {
            assert!(md.contains(s), "missing {s:?}");
        }
        let h = render_character(&vars().actor);
        for s in [
            "Human scout",
            "human",
            "sight: 15 m, 60 degrees to each side",
            "hearing: 10 m",
            "1.4 m/s",
            "`use_tool_on`",
            "`craft`",
            "`mount`",
        ] {
            assert!(h.contains(s), "missing {s:?}");
        }
    }

    #[test]
    fn agents_md_lists_reach() {
        let md = render_character(&vars().actor);
        assert!(
            md.contains(
                "- reach: 1.5 m all around, through nothing solid
"
            ),
            "{md}"
        );
        // A sense line, not a tool manual.
        assert!(!md.contains("within_reach"), "{md}");
    }

    #[test]
    fn agents_md_handles_an_actor_without_senses_or_actions() {
        let mut v = vars();
        v.actor.sensors.clear();
        v.actor.actions.clear();
        let md = render_character(&v.actor);
        assert!(md.contains("Senses:\n\n- none\n"), "{md}");
        assert!(
            md.contains(
                "Actions: none.
"
            ),
            "{md}"
        );
        assert!(!md.contains("Actions: ."), "{md}");
    }

    #[test]
    fn agents_md_notes_are_only_through_the_tools() {
        let md = render_agents_md(&vars());
        for s in [
            "diary: what happened",
            "memory: lasting facts",
            "proposals: ideas for the dev",
            "never in files",
            "Only the dev sets a proposal's `status`",
        ] {
            assert!(md.contains(s), "missing {s:?}");
        }
        for s in [
            "diary.md",
            "memory.md",
            "proposals/",
            "Create the file",
            "Append-only",
        ] {
            assert!(!md.contains(s), "still says {s:?}");
        }
    }

    #[test]
    fn agents_md_has_credentials_and_no_cli_unit_controller_or_url() {
        let v = vars();
        let md = render_agents_md(&v);
        assert!(md.contains(&v.identity.agent_id) && md.contains(&v.identity.token));
        for forbidden in [
            "npcctl",
            "controller",
            "actor #33",
            "--unit",
            "http://127.0.0.1",
            "Without MCP",
            "Human scout",
            "sight: 15 m",
        ] {
            assert!(
                !md.contains(forbidden),
                "forbidden bootstrap detail {forbidden}"
            );
        }
        let register = md.find("`register`").unwrap();
        let enter = md.find("`enter_world`").unwrap();
        let take = md.find("`take_control`").unwrap();
        assert!(register < enter && enter < take);
        assert!(!format!("{v:?}").contains(&v.identity.token));
        for kind in [
            PromptKind::Start,
            PromptKind::Resume,
            PromptKind::ResumeChanged,
            PromptKind::ResumeRebuilt,
        ] {
            assert!(!render_prompt(kind, &v.prompt).contains(&v.identity.token));
        }
    }

    #[test]
    fn agents_md_keeps_the_rules_and_spoils_nothing() {
        let md = render_agents_md(&vars());
        let rules = &md[md.find("## Rules").unwrap()..];
        for s in [
            "Sense and act only through the game's MCP tools",
            "Never change the game speed",
            "alter the simulation directly",
            "edit game files",
            "Do not record your token",
        ] {
            assert!(rules.contains(s), "{s}");
        }
        for hidden in ["world.mutate_", "npc/command", "npc/pause", "npc/step"] {
            assert!(!md.contains(hidden), "hidden developer API {hidden}");
        }
        // No real store ids and no fixed date to copy.
        assert!(!md.contains("@26") && !md.contains("2026-"), "{md}");
        assert!(!md.contains("not built yet"));
        let mut v = vars();
        v.prompt.npcctl_missing = true;
        let m = render_agents_md(&v);
        assert!(!m.contains("npcctl"));
    }

    #[test]
    fn agents_md_fills_every_placeholder() {
        let md = render_agents_md(&vars());
        let left = md.match_indices('{').find_map(|(i, _)| {
            let rest = &md[i + 1..];
            let name = &rest[..rest.find('}')?];
            (!name.is_empty() && name.bytes().all(|c| c.is_ascii_uppercase() || c == b'_'))
                .then(|| name.to_string())
        });
        assert_eq!(left, None);
        assert!(md.len() > 2000 && md.len() < 5000, "{} bytes", md.len());
    }
}
