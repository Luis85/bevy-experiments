//! Agent folders: `<agents folder>/<slug(agent)>/` holding the generated
//! workspace (`AGENTS.md`, `prompts/`, the Claude Code config,
//! [`super::workspace`]) and the owned files: the notes (`diary/`,
//! `memory/`, `proposals/`: one Obsidian-compatible markdown file per entry,
//! written only through the game's notes store, plus `diary.md.bak` /
//! `memory.md.bak` after an old folder migrated), `map/`, the world save,
//! `agent.json` and `run.json`. The folder can be opened as an Obsidian
//! vault.

use std::path::{Path, PathBuf};

use super::prompt::PromptKind;

use serde::{Deserialize, Serialize};

/// The text of `path`; missing or unreadable gives empty, invalid UTF-8 is
/// converted lossily, a leading BOM is dropped (`agent.json`, the menu's
/// settings).
pub fn read_lossy(path: &Path) -> String {
    std::fs::read(path)
        .map(|bytes| {
            String::from_utf8_lossy(&bytes)
                .trim_start_matches('\u{feff}')
                .to_string()
        })
        .unwrap_or_default()
}

/// A file-system-safe name: ASCII lowercase letters and digits, every other run
/// becomes one `-`, trimmed, at most 48 chars; empty gives `unnamed`.
pub fn slug(name: &str) -> String {
    let mut out = String::new();
    let mut dash = false;
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            if dash && !out.is_empty() {
                out.push('-');
            }
            dash = false;
            out.push(c.to_ascii_lowercase());
        } else {
            dash = true;
        }
    }
    out.truncate(48);
    let out = out.trim_end_matches('-').to_string();
    if out.is_empty() {
        return "unnamed".to_string();
    }
    let reserved = matches!(out.as_str(), "con" | "prn" | "aux" | "nul")
        || ((out.starts_with("com") || out.starts_with("lpt"))
            && out.len() == 4
            && matches!(out.as_bytes()[3], b'1'..=b'9'));
    if reserved {
        format!("agent-{out}")
    } else {
        out
    }
}

/// Why `name` cannot name an agent folder, or `None` when it can: blank, or
/// no ASCII letter or digit for [`slug`] to keep.
pub fn name_problem(name: &str) -> Option<String> {
    if name.trim().is_empty() {
        Some("enter a name".to_string())
    } else if !name.chars().any(|c| c.is_ascii_alphanumeric()) {
        Some("the name has no letters or digits a folder can use".to_string())
    } else {
        None
    }
}

/// One agent's folder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentDir {
    /// The folder.
    pub dir: PathBuf,
}

impl AgentDir {
    /// The prompt file of `kind`: `prompts/start.md`, `prompts/resume.md`,
    /// `prompts/resume_changed.md` or `prompts/resume_rebuilt.md` (generated,
    /// [`super::workspace::write_workspace`]).
    pub fn prompt(&self, kind: PromptKind) -> PathBuf {
        let name = match kind {
            PromptKind::Start => "start.md",
            PromptKind::Resume => "resume.md",
            PromptKind::ResumeChanged => "resume_changed.md",
            PromptKind::ResumeRebuilt => "resume_rebuilt.md",
        };
        self.dir.join("prompts").join(name)
    }
    /// `diary/`: one file per diary entry.
    pub fn diary(&self) -> PathBuf {
        self.dir.join("diary")
    }
    /// `memory/`: one file per memory.
    pub fn memory(&self) -> PathBuf {
        self.dir.join("memory")
    }
    /// `proposals/`: one file per proposal.
    pub fn proposals(&self) -> PathBuf {
        self.dir.join("proposals")
    }
    /// `agent.json`.
    pub fn meta(&self) -> PathBuf {
        self.dir.join("agent.json")
    }
    /// `<slug(world)>.world.json`.
    pub fn save(&self, world: &str) -> PathBuf {
        self.dir.join(format!("{}.world.json", slug(world)))
    }
    /// Renames the save to `<slug>.world.<date>.json` and returns the new
    /// path. Never overwrites a backup: if that name is taken, `-2`, `-3`, ..
    /// is appended to the date.
    pub fn backup_save(&self, world: &str, date: &str) -> std::io::Result<PathBuf> {
        let (w, d) = (slug(world), slug(date));
        let mut to = self.dir.join(format!("{w}.world.{d}.json"));
        let mut n = 2;
        while to.exists() {
            to = self.dir.join(format!("{w}.world.{d}-{n}.json"));
            n += 1;
        }
        std::fs::rename(self.save(world), &to)?;
        Ok(to)
    }
}

/// The contents of `agent.json`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct AgentMeta {
    /// The agent's name.
    pub agent: String,
    /// The world's name.
    pub world: String,
    /// Creation date, as passed by the caller.
    pub created: String,
    /// The actor preset's id (`human`, `rover`, ..; `crate::actors`).
    /// Absent in folders from before presets: the human.
    #[serde(default = "default_preset")]
    pub preset: String,
}

fn default_preset() -> String {
    crate::actors::HUMAN.to_owned()
}

/// Creates the folder, the empty notes folders `diary/`, `memory/` and
/// `proposals/`, and `agent.json`. Refuses a folder that already exists; a
/// failure part-way removes the new folder.
pub fn create_agent(
    root: &Path,
    agent: &str,
    world: &str,
    created: &str,
) -> Result<AgentDir, String> {
    create_agent_as(root, agent, world, created, crate::actors::HUMAN)
}

/// [`create_agent`] for an agent playing actor preset `preset` (its id,
/// stored in `agent.json`).
pub fn create_agent_as(
    root: &Path,
    agent: &str,
    world: &str,
    created: &str,
    preset: &str,
) -> Result<AgentDir, String> {
    if let Some(problem) = name_problem(agent) {
        return Err(problem);
    }
    let fill = |ad: &AgentDir, agent: &str, world: &str, created: &str| {
        fill_agent(ad, agent, world, created, preset)
    };
    create_agent_with(root, agent, world, created, &fill)
}

fn fill_agent(
    ad: &AgentDir,
    agent: &str,
    world: &str,
    created: &str,
    preset: &str,
) -> Result<(), String> {
    let err = |e: std::io::Error| format!("cannot write {}: {e}", ad.dir.display());
    for notes in [ad.diary(), ad.memory(), ad.proposals()] {
        std::fs::create_dir(notes).map_err(err)?;
    }
    let meta = AgentMeta {
        agent: agent.into(),
        world: world.into(),
        created: created.into(),
        preset: preset.into(),
    };
    let json = serde_json::to_string_pretty(&meta).map_err(|e| e.to_string())?;
    std::fs::write(ad.meta(), json).map_err(err)
}

type Fill<'a> = dyn Fn(&AgentDir, &str, &str, &str) -> Result<(), String> + 'a;

fn create_agent_with(
    root: &Path,
    agent: &str,
    world: &str,
    created: &str,
    fill: &Fill<'_>,
) -> Result<AgentDir, String> {
    std::fs::create_dir_all(root)
        .map_err(|e| format!("cannot create the agents folder {}: {e}", root.display()))?;
    let dir = root.join(slug(agent));
    match std::fs::create_dir(&dir) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            return Err(format!("an agent folder already exists: {}", dir.display()));
        }
        Err(e) => return Err(format!("cannot write {}: {e}", dir.display())),
    }
    let ad = AgentDir { dir };
    if let Err(e) = fill(&ad, agent, world, created) {
        let _ = std::fs::remove_dir_all(&ad.dir);
        return Err(e);
    }
    Ok(ad)
}

/// A listed agent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentEntry {
    /// Its `agent.json`.
    pub meta: AgentMeta,
    /// Its folder.
    pub dir: AgentDir,
    /// Whether the world save exists.
    pub has_save: bool,
}

/// Agents under `root`, sorted by agent name; folders without a readable
/// `agent.json` are skipped (a leading BOM is tolerated).
pub fn list_agents(root: &Path) -> Vec<AgentEntry> {
    let Ok(rd) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    let mut out: Vec<AgentEntry> = rd
        .filter_map(Result::ok)
        .filter_map(|e| {
            let dir = AgentDir { dir: e.path() };
            let meta: AgentMeta = serde_json::from_str(&read_lossy(&dir.meta())).ok()?;
            let has_save = dir.save(&meta.world).is_file();
            Some(AgentEntry {
                meta,
                dir,
                has_save,
            })
        })
        .collect();
    out.sort_by(|a, b| {
        a.meta
            .agent
            .cmp(&b.meta.agent)
            .then_with(|| a.dir.dir.cmp(&b.dir.dir))
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_lossy_tolerates_missing_bad_utf8_and_bom() {
        let d = std::env::temp_dir().join(format!("npc-read-lossy-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        assert_eq!(read_lossy(&d.join("missing")), "");
        std::fs::write(d.join("bad"), [0xff, 0xfe, 0xfd]).unwrap();
        assert_eq!(read_lossy(&d.join("bad")).chars().count(), 3);
        std::fs::write(d.join("bom"), "\u{feff}{}").unwrap();
        assert_eq!(read_lossy(&d.join("bom")), "{}");
        let _ = std::fs::remove_dir_all(&d);
    }

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("npc-home-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn slug_strips_illegal_characters() {
        assert_eq!(slug("a/b\\c:d*e?f\"g<h>i|j"), "a-b-c-d-e-f-g-h-i-j");
        assert_eq!(slug("  Wren Ashdown! "), "wren-ashdown");
    }

    #[test]
    fn slug_of_blank_is_unnamed() {
        assert_eq!(slug(""), "unnamed");
        assert_eq!(slug("  /// "), "unnamed");
    }

    #[test]
    fn slug_avoids_windows_reserved_names() {
        assert_eq!(slug("CON"), "agent-con");
        assert_eq!(slug("com1"), "agent-com1");
        assert_eq!(slug("Lpt9"), "agent-lpt9");
        assert_eq!(slug("nul"), "agent-nul");
        assert_eq!(slug("com0"), "com0");
        assert_eq!(slug("console"), "console");
    }

    #[test]
    fn create_agent_cleans_up_on_failure() {
        // No portable way to make a real step fail, so inject a failing fill.
        let d = tmp("cleanup");
        let fail = |ad: &AgentDir, _: &str, _: &str, _: &str| {
            std::fs::create_dir(ad.proposals()).unwrap();
            Err("boom".to_string())
        };
        let e = create_agent_with(&d, "Wren", "W", "t", &fail).unwrap_err();
        assert_eq!(e, "boom");
        assert!(!d.join("wren").exists());
        assert!(create_agent(&d, "Wren", "W", "t").is_ok());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn slug_caps_length() {
        let s = slug(&format!("{} b", "a".repeat(60)));
        assert_eq!(s.len(), 48);
        let s = slug(&format!("{} b", "a".repeat(47)));
        assert_eq!(s, "a".repeat(47));
    }

    #[test]
    fn create_agent_writes_layout() {
        let d = tmp("layout");
        let a = create_agent(&d, "Wren Ashdown", "Hollowmere", "2026-10-24").unwrap();
        assert_eq!(a.dir, d.join("wren-ashdown"));
        assert_eq!(
            (a.diary(), a.memory(), a.proposals()),
            (
                a.dir.join("diary"),
                a.dir.join("memory"),
                a.dir.join("proposals")
            )
        );
        for notes in [a.diary(), a.memory(), a.proposals()] {
            assert!(notes.is_dir(), "{}", notes.display());
            assert_eq!(std::fs::read_dir(&notes).unwrap().count(), 0);
        }
        assert!(!a.dir.join("diary.md").exists(), "no one-file diary");
        let m: AgentMeta =
            serde_json::from_str(&std::fs::read_to_string(a.meta()).unwrap()).unwrap();
        assert_eq!(m.world, "Hollowmere");
        assert_eq!(a.save("Hollowmere"), a.dir.join("hollowmere.world.json"));
        std::fs::write(a.save("Hollowmere"), "{}").unwrap();
        let b = a.backup_save("Hollowmere", "2026-10-25").unwrap();
        assert_eq!(b, a.dir.join("hollowmere.world.2026-10-25.json"));
        assert!(b.is_file() && !a.save("Hollowmere").exists());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn name_problem_explains_unusable_names() {
        assert_eq!(name_problem(""), Some("enter a name".into()));
        assert_eq!(name_problem("   "), Some("enter a name".into()));
        let msg = "the name has no letters or digits a folder can use";
        assert_eq!(name_problem("///"), Some(msg.into()));
        assert_eq!(name_problem("\u{e9}\u{20ac}"), Some(msg.into()));
        assert_eq!(name_problem("Wren"), None);
        assert_eq!(name_problem("unnamed"), None);
        assert_eq!(name_problem("  7 "), None);
    }

    #[test]
    fn create_agent_refuses_unusable_names() {
        let d = tmp("badname");
        let e = create_agent(&d, "///", "W", "t").unwrap_err();
        assert_eq!(e, "the name has no letters or digits a folder can use");
        assert_eq!(create_agent(&d, "", "W", "t").unwrap_err(), "enter a name");
        assert!(!d.join("unnamed").exists());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn backup_save_never_overwrites_a_backup() {
        let d = tmp("backup");
        let a = create_agent(&d, "Wren", "Hollowmere", "t").unwrap();
        let mut got = Vec::new();
        for i in 0..3 {
            std::fs::write(a.save("Hollowmere"), format!("save {i}")).unwrap();
            got.push(a.backup_save("Hollowmere", "2026-10-25").unwrap());
        }
        let names: Vec<String> = got
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            names,
            [
                "hollowmere.world.2026-10-25.json",
                "hollowmere.world.2026-10-25-2.json",
                "hollowmere.world.2026-10-25-3.json"
            ]
        );
        for (i, p) in got.iter().enumerate() {
            assert_eq!(std::fs::read_to_string(p).unwrap(), format!("save {i}"));
        }
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn create_agent_refuses_existing() {
        let d = tmp("exists");
        create_agent(&d, "Wren Ashdown", "W", "t").unwrap();
        let e = create_agent(&d, "wren  ashdown", "W", "t").unwrap_err();
        assert!(e.contains("wren-ashdown"), "{e}");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn create_agent_reports_unwritable_root() {
        let d = tmp("unwritable");
        let file = d.join("a-file");
        std::fs::write(&file, "x").unwrap();
        let e = create_agent(&file, "Wren", "W", "t").unwrap_err();
        assert!(e.contains("a-file"), "{e}");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn list_agents_reads_a_bom_meta() {
        let d = tmp("list-bom");
        let a = create_agent(&d, "Wren Ashdown", "Hollowmere", "2026-10-24").unwrap();
        let text = std::fs::read_to_string(a.meta()).unwrap();
        std::fs::write(a.meta(), format!("\u{feff}{text}")).unwrap();
        let listed = list_agents(&d);
        assert_eq!(listed.len(), 1, "{listed:?}");
        assert_eq!(listed[0].meta.agent, "Wren Ashdown");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn list_agents_skips_folders_without_meta() {
        let d = tmp("list");
        create_agent(&d, "Zed", "W1", "t").unwrap();
        let b = create_agent(&d, "Amy", "W2", "t").unwrap();
        std::fs::write(b.save("W2"), "{}").unwrap();
        std::fs::create_dir(d.join("stray")).unwrap();
        std::fs::create_dir(d.join("broken")).unwrap();
        std::fs::write(d.join("broken").join("agent.json"), "nope").unwrap();
        let l = list_agents(&d);
        let names: Vec<&str> = l.iter().map(|e| e.meta.agent.as_str()).collect();
        assert_eq!(names, ["Amy", "Zed"]);
        assert!(l[0].has_save && !l[1].has_save);
        let _ = std::fs::remove_dir_all(&d);
    }
}
