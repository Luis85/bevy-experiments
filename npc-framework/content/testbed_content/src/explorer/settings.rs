//! App settings: where the agent folders live, and whether explorer
//! windows draw the fog of war.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::home::read_lossy;

/// Persistent settings of the game menu.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Settings {
    /// The folder holding one sub-folder per agent.
    pub agents_dir: PathBuf,
    /// Explorer windows draw the fog of war (spec G8; `actor_testbed --fog
    /// on|off`). Default on; absent in older files: on.
    #[serde(default = "fog_default")]
    pub fog_of_war: bool,
    /// The actor presets folder (spec G11, `crate::actors`); `None` (absent
    /// in older files): [`default_actors_dir`] of the agents folder.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actors_dir: Option<PathBuf>,
}

/// The actor presets folder beside the agents folder:
/// `<agents folder>/../npc-actors` (spec G11; `npc-actors` inside the agents
/// folder when it has no parent). The built-in presets are copied into it on
/// first use (`crate::actors::load_presets`).
pub fn default_actors_dir(agents_dir: &Path) -> PathBuf {
    agents_dir
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map_or_else(|| agents_dir.join("npc-actors"), |p| p.join("npc-actors"))
}

fn fog_default() -> bool {
    true
}

impl Settings {
    /// Defaults: `<home>/npc-agents`, fog of war on.
    pub fn default_for(home: &Path) -> Self {
        Self {
            agents_dir: home.join("npc-agents"),
            fog_of_war: true,
            actors_dir: None,
        }
    }

    /// The actor presets folder: the setting, else [`default_actors_dir`].
    pub fn actors_dir(&self) -> PathBuf {
        self.actors_dir
            .clone()
            .unwrap_or_else(|| default_actors_dir(&self.agents_dir))
    }

    /// The settings file: `APPDATA`, else `XDG_CONFIG_HOME`, else `HOME/.config`,
    /// plus `npc-framework/settings.json`.
    pub fn file(env: &dyn Fn(&str) -> Option<String>) -> Option<PathBuf> {
        let nonempty = |k: &str| env(k).filter(|v| !v.is_empty());
        let base = if let Some(a) = nonempty("APPDATA") {
            PathBuf::from(a)
        } else if let Some(x) = nonempty("XDG_CONFIG_HOME") {
            PathBuf::from(x)
        } else {
            PathBuf::from(nonempty("HOME")?).join(".config")
        };
        Some(base.join("npc-framework").join("settings.json"))
    }

    /// Reads `path` (a leading BOM is tolerated); a missing or malformed
    /// file gives the default.
    pub fn load(path: &Path, home: &Path) -> Self {
        serde_json::from_str(&read_lossy(path)).unwrap_or_else(|_| Self::default_for(home))
    }

    /// Writes `path`, creating parent folders.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let text = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        std::fs::write(path, text)
    }
}

/// Whether a read-only flag makes a path unusable. On Windows the flag on a
/// folder is a shell hint (set on Documents, OneDrive and customised
/// folders) that is not enforced, so it is ignored for folders there.
fn readonly_blocks(is_dir: bool, readonly: bool, windows: bool) -> bool {
    readonly && !(is_dir && windows)
}

/// `p` is an absolute folder that exists and is writable, or can be created
/// (its nearest existing ancestor is a writable folder). Never touches the
/// file system beyond reading metadata.
pub fn validate_agents_dir(p: &Path) -> Result<(), String> {
    if p.as_os_str().is_empty() {
        return Err("the agents folder is empty".into());
    }
    if !p.is_absolute() {
        return Err(format!("use an absolute folder, not {}", p.display()));
    }
    let mut cur = p;
    loop {
        if let Ok(md) = std::fs::metadata(cur) {
            if !md.is_dir() {
                return Err(format!("{} is not a folder", cur.display()));
            }
            if readonly_blocks(md.is_dir(), md.permissions().readonly(), cfg!(windows)) {
                return Err(format!("{} is read-only", cur.display()));
            }
            return Ok(());
        }
        match cur.parent() {
            Some(parent) => cur = parent,
            None => return Err(format!("{} does not exist", p.display())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("npc-settings-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn settings_round_trip_and_fallback() {
        let d = tmp("rt");
        let home = d.join("home");
        let f = d.join("cfg").join("settings.json");
        assert_eq!(Settings::load(&f, &home), Settings::default_for(&home));
        let s = Settings {
            agents_dir: d.join("agents"),
            fog_of_war: false,
            actors_dir: Some(d.join("my-actors")),
        };
        s.save(&f).unwrap();
        assert_eq!(Settings::load(&f, &home), s);
        std::fs::write(&f, "{ not json").unwrap();
        assert_eq!(Settings::load(&f, &home), Settings::default_for(&home));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn fog_of_war_defaults_on() {
        let d = tmp("fog");
        let home = d.join("home");
        assert!(Settings::default_for(&home).fog_of_war);
        // A file from before the setting existed: fog on.
        let f = d.join("settings.json");
        std::fs::write(&f, r#"{"agents_dir":"/x/agents"}"#).unwrap();
        let s = Settings::load(&f, &home);
        assert_eq!(s.agents_dir, PathBuf::from("/x/agents"));
        assert!(s.fog_of_war);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn actors_dir_defaults_beside_the_agents_folder() {
        let d = tmp("actors");
        let home = d.join("home");
        let s = Settings::default_for(&home);
        assert_eq!(s.actors_dir(), home.join("npc-actors"));
        // A file from before the setting: the default; a set one wins.
        let f = d.join("settings.json");
        std::fs::write(&f, r#"{"agents_dir":"/x/agents"}"#).unwrap();
        let s = Settings::load(&f, &home);
        assert_eq!(s.actors_dir, None);
        assert_eq!(s.actors_dir(), Path::new("/x").join("npc-actors"));
        let s = Settings {
            actors_dir: Some(d.join("mine")),
            ..s
        };
        assert_eq!(s.actors_dir(), d.join("mine"));
        assert_eq!(
            default_actors_dir(Path::new("agents")),
            Path::new("agents").join("npc-actors")
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn settings_file_prefers_appdata() {
        let env = |pairs: &'static [(&'static str, &'static str)]| {
            move |k: &str| {
                pairs
                    .iter()
                    .find(|(n, _)| *n == k)
                    .map(|(_, v)| v.to_string())
            }
        };
        let both = env(&[("APPDATA", "/a"), ("XDG_CONFIG_HOME", "/x"), ("HOME", "/h")]);
        let want = |b: &str| Path::new(b).join("npc-framework").join("settings.json");
        assert_eq!(Settings::file(&both), Some(want("/a")));
        let xdg = env(&[("XDG_CONFIG_HOME", "/x"), ("HOME", "/h")]);
        assert_eq!(Settings::file(&xdg), Some(want("/x")));
        let home = env(&[("HOME", "/h")]);
        assert_eq!(Settings::file(&home), Some(want("/h/.config")));
        assert_eq!(Settings::file(&env(&[])), None);
    }

    #[test]
    fn validate_rejects_empty_and_relative() {
        assert!(validate_agents_dir(Path::new("")).is_err());
        let e = validate_agents_dir(Path::new("agents/here")).unwrap_err();
        assert!(e.contains("absolute"), "{e}");
    }

    #[test]
    fn validate_accepts_creatable_missing_folder_without_creating_it() {
        let d = tmp("creatable");
        let missing = d.join("a").join("b");
        assert!(validate_agents_dir(&d).is_ok());
        assert!(validate_agents_dir(&missing).is_ok());
        assert!(!d.join("a").exists());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn settings_load_tolerates_a_bom() {
        let d = tmp("bom");
        let home = d.join("home");
        let f = d.join("settings.json");
        let s = Settings {
            agents_dir: d.join("agents"),
            fog_of_war: true,
            actors_dir: None,
        };
        let json = serde_json::to_string(&s).unwrap();
        std::fs::write(&f, format!("\u{feff}{json}")).unwrap();
        assert_eq!(Settings::load(&f, &home), s);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn readonly_blocks_only_where_it_is_enforced() {
        // Windows sets the read-only attribute on Documents, OneDrive and
        // customised folders, and does not enforce it for folders.
        assert!(!readonly_blocks(true, true, true));
        assert!(readonly_blocks(true, true, false));
        assert!(readonly_blocks(false, true, true));
        assert!(!readonly_blocks(true, false, false));
        assert!(!readonly_blocks(false, false, true));
    }

    #[cfg(windows)]
    #[test]
    fn validate_accepts_a_windows_readonly_folder() {
        let d = tmp("readonly");
        let set = |readonly: bool| {
            let mut perm = std::fs::metadata(&d).unwrap().permissions();
            perm.set_readonly(readonly);
            std::fs::set_permissions(&d, perm).unwrap();
        };
        set(true);
        assert!(std::fs::metadata(&d).unwrap().permissions().readonly());
        let result = validate_agents_dir(&d);
        set(false);
        assert_eq!(result, Ok(()));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn validate_rejects_a_file_path() {
        let d = tmp("filepath");
        let f = d.join("f");
        std::fs::write(&f, "x").unwrap();
        let e = validate_agents_dir(&f).unwrap_err();
        assert!(e.contains("not a folder"), "{e}");
        let e = validate_agents_dir(&f.join("sub")).unwrap_err();
        assert!(e.contains(&f.display().to_string()), "{e}");
        let _ = std::fs::remove_dir_all(&d);
    }
}
