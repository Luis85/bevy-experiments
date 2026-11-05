//! The active profile: where its campaign and settings live, and the startup
//! read that loads them. Construction of the frontend itself stays pure.
use crate::{launch::LaunchConfig, settings::Settings};
use bevy::prelude::Resource;
use horde_core::sim::Game;
use std::path::{Path, PathBuf};

/// One profile's `campaign.json` and `settings.json`.
#[derive(Resource, Clone, Debug)]
pub struct ProfilePaths {
    campaign: PathBuf,
    settings: PathBuf,
}
impl ProfilePaths {
    pub fn in_directory(directory: PathBuf) -> Self {
        Self {
            campaign: directory.join("campaign.json"),
            settings: directory.join("settings.json"),
        }
    }
    /// Explicit files, for tests that make one of them unwritable.
    #[cfg(test)]
    pub fn files(campaign: PathBuf, settings: PathBuf) -> Self {
        Self { campaign, settings }
    }
    /// The save location for this launch: the configured save directory, or a
    /// practice/review profile beneath or beside it.
    pub fn from_config(config: &LaunchConfig) -> Self {
        Self::in_directory(profile_directory(
            config.save_directory.clone(),
            config.scenario.as_deref(),
            config.practice,
        ))
    }
    pub fn campaign(&self) -> PathBuf {
        self.campaign.clone()
    }
    pub fn settings(&self) -> PathBuf {
        self.settings.clone()
    }
}

impl LoadedProfile {
    /// Reads the campaign and settings. Unreadable saves degrade to a fresh
    /// in-memory game plus a warning, so loading itself cannot fail.
    pub fn load(paths: &ProfilePaths) -> Self {
        let directory = paths
            .campaign
            .parent()
            .map(PathBuf::from)
            .unwrap_or_default();
        load_profile(directory)
    }
}

pub struct LoadedProfile {
    pub game: Game,
    pub settings: Settings,
    pub warning: String,
}
/// Keep resumable practice profiles separate; screenshot fixtures are ephemeral.
pub fn profile_directory(base: PathBuf, scenario: Option<&str>, practice: bool) -> PathBuf {
    if practice {
        let tag: String = scenario
            .unwrap_or("battle")
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
            .take(48)
            .collect();
        base.join("practice")
            .join(if tag.is_empty() { "battle" } else { &tag })
    } else if scenario.is_some() {
        std::env::temp_dir().join(format!("lastlight-review-{}", std::process::id()))
    } else {
        base
    }
}
/// Startup read service, separate from widget layout and the command runner.
pub fn load_profile(directory: PathBuf) -> LoadedProfile {
    let campaign_path = directory.join("campaign.json");
    let settings_path = directory.join("settings.json");
    let (game, warning) = if campaign_path.exists() || campaign_path.with_extension("bak").exists()
    {
        match Game::load(&campaign_path) {
            Ok(game) => (
                game,
                "Campaign restored. Continue to resume the exact deployment.".into(),
            ),
            Err(error) => (
                Game::default(),
                format!(
                    "Save could not be read: {error}. Starting fresh in memory. Original files remain unchanged; unreadable generations will be preserved as recovery copies before any new save replaces them."
                ),
            ),
        }
    } else {
        (Game::default(), String::new())
    };
    LoadedProfile {
        game,
        settings: read_settings(&settings_path).settings,
        warning,
    }
}

/// The startup read of one `settings.json`. The warning is logged here; the
/// settings-unreadable modal (wave 3) is what shows it on screen.
#[derive(Clone, Debug)]
pub struct SettingsLoad {
    pub settings: Settings,
    /// Set when any value fell back to its default.
    #[cfg_attr(not(test), allow(dead_code))]
    pub warning: Option<String>,
    /// Where the original bytes were copied before anything could replace them.
    #[cfg_attr(not(test), allow(dead_code))]
    pub preserved: Option<PathBuf>,
}

/// The copy of an unreadable settings file, beside it.
pub const UNREADABLE_SETTINGS: &str = "settings.unreadable.json";

/// Reads `settings.json`. When any value falls back to its default, the
/// original bytes are first copied to `settings.unreadable.json` beside it,
/// before this returns and so before any save can replace them. A missing
/// file is a fresh profile: defaults, no warning.
pub fn read_settings(path: &Path) -> SettingsLoad {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return SettingsLoad {
                settings: Settings::default(),
                warning: None,
                preserved: None,
            };
        }
        Err(error) => {
            let warning = format!("Settings could not be read ({error}); defaults are in use.");
            bevy::log::warn!("{warning}");
            return SettingsLoad {
                settings: Settings::default(),
                warning: Some(warning),
                preserved: None,
            };
        }
    };
    let decoded = Settings::decode(&bytes);
    if !decoded.not_json && decoded.dropped_keys.is_empty() {
        return SettingsLoad {
            settings: decoded.settings,
            warning: None,
            preserved: None,
        };
    }
    let copy = path.with_file_name(UNREADABLE_SETTINGS);
    let (preserved, kept) = match write_synced(&copy, &bytes) {
        Ok(()) => (
            Some(copy),
            format!("The original is kept as {UNREADABLE_SETTINGS}."),
        ),
        Err(error) => (
            None,
            format!("The original could not be copied to {UNREADABLE_SETTINGS}: {error}."),
        ),
    };
    let warning = if decoded.not_json {
        format!("Settings could not be read; defaults are in use. {kept}")
    } else {
        format!(
            "Some settings could not be read ({}); they use defaults. {kept}",
            decoded.dropped_keys.join(", ")
        )
    };
    bevy::log::warn!("{warning}");
    SettingsLoad {
        settings: decoded.settings,
        warning: Some(warning),
        preserved,
    }
}
fn write_synced(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let mut file = std::fs::File::create(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    /// A fresh, created temp directory unique to this call.
    fn directory(name: &str) -> PathBuf {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let directory = std::env::temp_dir().join(format!(
            "lastlight-settings-{name}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).unwrap();
        directory
    }

    #[test]
    fn non_json_settings_file_is_preserved_before_defaults() {
        let directory = directory("non-json");
        let path = directory.join("settings.json");
        let original = b"{\"master\": 0.3, truncated by a crash";
        std::fs::write(&path, original).unwrap();
        let load = read_settings(&path);
        assert_eq!(load.settings, Settings::default());
        let copy = directory.join(UNREADABLE_SETTINGS);
        assert_eq!(load.preserved.as_deref(), Some(copy.as_path()));
        assert_eq!(std::fs::read(&copy).unwrap(), original);
        assert_eq!(
            load.warning.as_deref(),
            Some(
                "Settings could not be read; defaults are in use. The original is kept as settings.unreadable.json."
            )
        );
        // The original stays in place; only a later save replaces it, and the
        // copy outlives that save.
        assert_eq!(std::fs::read(&path).unwrap(), original);
        load.settings.save(&path).unwrap();
        assert_eq!(std::fs::read(&copy).unwrap(), original);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn dropped_key_preserves_original_and_warns() {
        let directory = directory("dropped");
        let path = directory.join("settings.json");
        let original = br#"{"master": 0.3, "effects": "loud", "blood": true}"#;
        std::fs::write(&path, original).unwrap();
        let load = read_settings(&path);
        assert_eq!(load.settings.master, 0.3);
        assert!(load.settings.blood);
        assert_eq!(load.settings.effects, Settings::default().effects);
        let copy = directory.join(UNREADABLE_SETTINGS);
        assert_eq!(load.preserved.as_deref(), Some(copy.as_path()));
        assert_eq!(std::fs::read(&copy).unwrap(), original);
        let warning = load.warning.expect("a dropped key warns");
        assert!(warning.contains("effects"), "{warning}");
        assert!(warning.contains(UNREADABLE_SETTINGS), "{warning}");
        load.settings.save(&path).unwrap();
        assert_eq!(std::fs::read(&copy).unwrap(), original);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn readable_or_missing_settings_neither_warn_nor_copy() {
        let directory = directory("clean");
        let path = directory.join("settings.json");
        let missing = read_settings(&path);
        assert_eq!(missing.settings, Settings::default());
        assert!(missing.warning.is_none() && missing.preserved.is_none());
        let saved = Settings {
            master: 0.25,
            ..Settings::default()
        };
        saved.save(&path).unwrap();
        let load = read_settings(&path);
        assert_eq!(load.settings, saved);
        assert!(load.warning.is_none() && load.preserved.is_none());
        assert!(!directory.join(UNREADABLE_SETTINGS).exists());
        std::fs::remove_dir_all(directory).unwrap();
    }
}
