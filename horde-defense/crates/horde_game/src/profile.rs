//! The active profile: where its campaign and settings live, and the startup
//! reads that load them. `settings.json` is read once, in `main` before the
//! App exists (`read_settings`); the campaign is read by `load_profile`, which
//! takes that settings read. Construction of the frontend itself stays pure.
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
    /// The profile directory: where `campaign.json` lives.
    pub fn directory(&self) -> PathBuf {
        self.campaign
            .parent()
            .map(PathBuf::from)
            .unwrap_or_default()
    }
    pub fn campaign(&self) -> PathBuf {
        self.campaign.clone()
    }
    pub fn settings(&self) -> PathBuf {
        self.settings.clone()
    }
}

impl LoadedProfile {
    /// Reads the campaign beside the startup settings read. Unreadable saves
    /// degrade to a fresh in-memory game plus a warning, so loading itself
    /// cannot fail.
    pub fn load(paths: &ProfilePaths, settings: SettingsLoad) -> Self {
        load_profile(paths.directory(), settings)
    }
}

pub struct LoadedProfile {
    pub game: Game,
    pub settings: Settings,
    pub warning: String,
    /// Why the save could not be read; the game is then a fresh in-memory
    /// one. The storage modal (ui/modal.rs) offers "Retry load" for it.
    pub load_error: Option<String>,
    /// The startup settings read's warning (`SettingsLoad::warning`): some or
    /// all settings fell back to defaults. The storage modal shows it.
    pub settings_warning: Option<String>,
    /// The startup read could neither use nor copy `settings.json`, so the
    /// first save must copy it before replacing it (`SettingsLoad::original_at_risk`).
    pub settings_at_risk: bool,
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
/// The status text of a journey read from its save.
pub const CAMPAIGN_RESTORED: &str = "Campaign restored. Continue to resume the exact deployment.";
/// Startup read service, separate from widget layout and the command runner.
/// Reads only `campaign.json`; `settings` is the one startup settings read.
pub fn load_profile(directory: PathBuf, settings: SettingsLoad) -> LoadedProfile {
    let campaign_path = directory.join("campaign.json");
    let (game, warning, load_error) = if campaign_path.exists()
        || campaign_path.with_extension("bak").exists()
    {
        match Game::load(&campaign_path) {
            Ok(game) => (game, CAMPAIGN_RESTORED.into(), None),
            Err(error) => (
                Game::default(),
                format!(
                    "Save could not be read: {error}. Starting fresh in memory. Original files remain unchanged; unreadable generations will be preserved as recovery copies before any new save replaces them."
                ),
                Some(error.to_string()),
            ),
        }
    } else {
        (Game::default(), String::new(), None)
    };
    LoadedProfile {
        game,
        load_error,
        settings_warning: settings.warning.clone(),
        settings_at_risk: settings.original_at_risk(),
        settings: settings.settings,
        warning,
    }
}

/// The startup read of one `settings.json`. `FrontendParts::load` logs the
/// warning once logging is up; the settings-unreadable modal (wave 3) is what
/// shows it on screen. The default is a fresh profile's read.
#[derive(Clone, Debug, Default)]
pub struct SettingsLoad {
    pub settings: Settings,
    /// Set when any value fell back to its default.
    pub warning: Option<String>,
    /// Where the original bytes were copied before anything could replace them.
    pub preserved: Option<PathBuf>,
}
impl SettingsLoad {
    /// Something fell back but no copy was made: the file could not be read,
    /// or the copy could not be written. Its first replacement must then
    /// copy it first (`preserve_settings_original`).
    pub fn original_at_risk(&self) -> bool {
        self.warning.is_some() && self.preserved.is_none()
    }
}

/// The copy of an unreadable settings file, beside it.
pub const UNREADABLE_SETTINGS: &str = "settings.unreadable.json";

/// Reads `settings.json`, once per process, before the App and its logging
/// exist (so this logs nothing). When any value falls back to its default,
/// the original bytes are first copied to `settings.unreadable.json` beside
/// it, before this returns and so before any save can replace them. A missing
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
    SettingsLoad {
        settings: decoded.settings,
        warning: Some(warning),
        preserved,
    }
}
/// Copies the `settings.json` about to be replaced to `settings.unreadable.json`
/// (synced), unless that copy already holds the same bytes. Called before each
/// save's settings write while the startup read's original is still uncopied;
/// an error holds back only that settings write (the campaign is already
/// saved, `Commit::SavedWithoutSettings`), so the original is never replaced
/// uncopied. A missing file needs no copy.
pub fn preserve_settings_original(path: &Path) -> Result<(), String> {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(format!(
                "Cannot preserve {} before replacement: {error}",
                path.display()
            ));
        }
    };
    let copy = path.with_file_name(UNREADABLE_SETTINGS);
    if std::fs::read(&copy).is_ok_and(|kept| kept == bytes) {
        return Ok(());
    }
    write_synced(&copy, &bytes).map_err(|error| {
        format!(
            "Cannot preserve {} as {UNREADABLE_SETTINGS} before replacement: {error}",
            path.display()
        )
    })
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
    fn unreadable_or_uncopied_settings_stay_at_risk_until_preserved() {
        let directory = directory("at-risk");
        let path = directory.join("settings.json");
        // A directory cannot be read as a file: an error other than NotFound.
        std::fs::create_dir(&path).unwrap();
        let load = read_settings(&path);
        assert_eq!(load.settings, Settings::default());
        assert!(load.warning.is_some() && load.preserved.is_none());
        assert!(load.original_at_risk());
        assert!(preserve_settings_original(&path).is_err());
        std::fs::remove_dir(&path).unwrap();
        // Nothing to preserve once the file is gone.
        assert_eq!(preserve_settings_original(&path), Ok(()));
        let copy = directory.join(UNREADABLE_SETTINGS);
        assert!(!copy.exists());
        // A copy made at startup leaves nothing at risk; a repeat is a no-op.
        std::fs::write(&path, b"not json").unwrap();
        let load = read_settings(&path);
        assert!(load.preserved.is_some() && !load.original_at_risk());
        assert_eq!(preserve_settings_original(&path), Ok(()));
        assert_eq!(std::fs::read(&copy).unwrap(), b"not json");
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
