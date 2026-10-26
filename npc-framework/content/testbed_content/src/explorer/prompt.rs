//! The prompt the game shows the dev to paste into an AI tool (spec §6).
//!
//! Templates live in `content/testbed_content/prompts/*.md` and are embedded
//! with `include_str!`; rendering fills the `{PLACEHOLDER}`s in one pass
//! ([`fill`]). The prompts are short: they send the agent to `AGENTS.md`
//! ([`super::workspace`]), which holds the mission, the character, the notes
//! and the rules.
//! Pure string work: no clock, no I/O.

use std::path::PathBuf;

/// The controller name the explorer scenario attaches.
pub const CONTROLLER: &str = "explorer";

const START: &str = include_str!("../../prompts/start.md");
const RESUME: &str = include_str!("../../prompts/resume.md");
const RESUME_CHANGED: &str = include_str!("../../prompts/resume_changed.md");
const RESUME_REBUILT: &str = include_str!("../../prompts/resume_rebuilt.md");
const COMMON: &str = include_str!("../../prompts/common.md");

/// Which opening the prompt gets; the common body follows every one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PromptKind {
    /// A new world.
    Start,
    /// A saved world, loaded.
    Resume,
    /// A saved world from an older game version, loaded anyway.
    ResumeChanged,
    /// The world was rebuilt from scratch; the diary was kept.
    ResumeRebuilt,
}

/// Everything the game fills in.
#[derive(Clone, Debug)]
pub struct PromptVars {
    /// The explorer's name.
    pub agent: String,
    /// The world's name.
    pub world: String,
    /// The game's remote URL.
    pub url: String,
    /// The controlled unit's id.
    pub unit: u32,
    /// Absolute path of the `npcctl` binary.
    pub npcctl: PathBuf,
    /// The agent's directory (diary and proposals live in it).
    pub dir: PathBuf,
    /// `npcctl` is not built yet.
    pub npcctl_missing: bool,
}

/// Render the prompt for `kind` with every placeholder filled: the kind's
/// opening, then the common part (both short: they send the agent to
/// `AGENTS.md`, [`super::workspace`]).
pub fn render_prompt(kind: PromptKind, v: &PromptVars) -> String {
    let head = match kind {
        PromptKind::Start => START,
        PromptKind::Resume => RESUME,
        PromptKind::ResumeChanged => RESUME_CHANGED,
        PromptKind::ResumeRebuilt => RESUME_REBUILT,
    };
    fill(&format!("{head}{COMMON}"), &prompt_values(v))
}

/// A path with forward slashes: command lines and paths in the text work in
/// bash, PowerShell and cmd, while a backslash path is mangled by bash.
fn slashed(p: &std::path::Path) -> String {
    p.display().to_string().replace('\\', "/")
}

/// The values of the `{PLACEHOLDER}`s every template may use.
pub(crate) fn prompt_values(v: &PromptVars) -> Vec<(&'static str, String)> {
    let note = if v.npcctl_missing {
        "npcctl is not built yet; the user must run `cargo build -p npcctl`.\n\n"
    } else {
        ""
    };
    vec![
        ("NPCCTL_NOTE", note.to_string()),
        ("NPCCTL", slashed(&v.npcctl)),
        ("DIR", slashed(&v.dir)),
        ("AGENTS_MD", slashed(&v.dir.join("AGENTS.md"))),
        ("AGENT", v.agent.clone()),
        ("WORLD", v.world.clone()),
        ("URL", v.url.clone()),
        ("UNIT", v.unit.to_string()),
        ("CONTROLLER", CONTROLLER.to_string()),
    ]
}

/// Replaces every `{NAME}` of `template` whose `NAME` is in `values`, in one
/// pass: a filled-in value is never scanned again, so an agent or world name
/// that looks like a placeholder stays as typed. Other braces (JSON examples)
/// are kept.
pub(crate) fn fill(template: &str, values: &[(&str, String)]) -> String {
    let mut out = String::with_capacity(template.len() + 256);
    let mut rest = template;
    while let Some(i) = rest.find('{') {
        out.push_str(&rest[..i]);
        let after = &rest[i + 1..];
        let value = after.find('}').and_then(|end| {
            let name = &after[..end];
            values
                .iter()
                .find(|(k, _)| *k == name)
                .map(|(_, v)| (v, end))
        });
        match value {
            Some((v, end)) => {
                out.push_str(v);
                rest = &after[end + 1..];
            }
            None => {
                out.push('{');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [PromptKind; 4] = [
        PromptKind::Start,
        PromptKind::Resume,
        PromptKind::ResumeChanged,
        PromptKind::ResumeRebuilt,
    ];

    fn vars(missing: bool) -> PromptVars {
        PromptVars {
            agent: "Wren Ashdown".into(),
            world: "Hollowmere".into(),
            url: "http://127.0.0.1:15702".into(),
            unit: 33,
            npcctl: PathBuf::from(r"C:\games\npcctl.exe"),
            dir: PathBuf::from(r"C:\agents\wren-ashdown"),
            npcctl_missing: missing,
        }
    }

    /// The first `{UPPER_NAME}` still in `s`, if any.
    fn placeholder_left(s: &str) -> Option<String> {
        s.match_indices('{').find_map(|(i, _)| {
            let rest = &s[i + 1..];
            let name = &rest[..rest.find('}')?];
            (!name.is_empty() && name.bytes().all(|c| c.is_ascii_uppercase() || c == b'_'))
                .then(|| name.to_string())
        })
    }

    #[test]
    fn every_variant_fills_every_placeholder() {
        for missing in [false, true] {
            for k in ALL {
                let p = render_prompt(k, &vars(missing));
                assert!(p.len() > 300, "{k:?} is empty");
                assert_eq!(placeholder_left(&p), None, "{k:?}");
            }
        }
    }

    #[test]
    fn fill_is_one_pass_and_keeps_other_braces() {
        let vals = [("A", "{B}".to_string()), ("B", "b".to_string())];
        assert_eq!(
            fill("{A} {B} {c} {\"k\": 1} {", &vals),
            "{B} b {c} {\"k\": 1} {"
        );
        let mut v = vars(false);
        v.agent = "{URL}".into();
        let p = render_prompt(PromptKind::Start, &v);
        assert!(p.contains("{URL}, you wake"), "{p}");
    }

    #[test]
    fn prompts_point_at_agents_md() {
        for k in ALL {
            let p = render_prompt(k, &vars(false));
            for s in [
                "Wren Ashdown",
                "Hollowmere",
                "`C:/agents/wren-ashdown/AGENTS.md`",
                "Epoch 1",
                "survive, learn, explore",
                "map",
                "actor #33",
                "the `notes_*` tools",
                "trust this folder",
            ] {
                assert!(p.contains(s), "{k:?} misses {s:?}");
            }
            // AGENTS.md is read before anything is done.
            let read = p.find("AGENTS.md").unwrap();
            assert!(read < p.find("observe").unwrap(), "{k:?}");
            assert!(!p.contains('\\'), "{k:?} has a backslash");
            // A mission brief, not a manual: the agent finds out how the
            // tools work itself.
            for s in ["act_and_wait", "expect_tick", "since_seq"] {
                assert!(!p.contains(s), "{k:?} teaches {s:?}");
            }
            assert!(p.len() < 1500, "{k:?} is {} bytes", p.len());
        }
    }

    #[test]
    fn resume_lists_memory_first() {
        for k in [
            PromptKind::Resume,
            PromptKind::ResumeChanged,
            PromptKind::ResumeRebuilt,
        ] {
            let p = render_prompt(k, &vars(false));
            let agents = p.find("AGENTS.md").expect("agents");
            let memory = p
                .find(r#"notes_list` with `book: "memory"`"#)
                .expect("memory");
            let diary = p.find(r#"`book: "diary"`"#).expect("diary");
            let proposals = p.find(r#"`book: "proposals"`"#).expect("proposals");
            let observe = p.find("Then observe").expect("then observe");
            assert!(agents < memory && memory < diary, "{k:?}");
            assert!(diary < proposals && proposals < observe, "{k:?}");
        }
        let c = render_prompt(PromptKind::ResumeChanged, &vars(false));
        assert!(c.contains("saved by another version of the game and may have changed"));
        let r = render_prompt(PromptKind::ResumeRebuilt, &vars(false));
        assert!(r.contains("rebuilt from scratch"));
        let s = render_prompt(PromptKind::Start, &vars(false));
        assert!(!s.contains("notes_list"), "{s}");
    }

    #[test]
    fn no_file_writing_instructions_remain() {
        for missing in [false, true] {
            for k in ALL {
                let p = render_prompt(k, &vars(missing));
                for s in [
                    "diary.md",
                    "memory.md",
                    "proposals/",
                    "Create the file",
                    "Append-only",
                    "NNN-",
                    "write it down as one file",
                ] {
                    assert!(!p.contains(s), "{k:?} still says {s:?}");
                }
                assert!(p.contains("never in files"), "{k:?}");
            }
        }
    }
}
