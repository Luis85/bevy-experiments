//! Startup inputs for `Frontend`, split so construction itself stays pure:
//! environment reads live in `StartupConfig`/`ProfilePaths::from_env`,
//! filesystem reads in `LoadedProfile::load`, and `Frontend::new` touches neither.
use crate::{
    runner::{self, LoadedProfile},
    settings,
};
use std::path::PathBuf;

/// Every environment switch consulted while the frontend starts.
#[derive(Clone, Debug, Default)]
pub struct StartupConfig {
    pub practice: bool,
    pub scenario: Option<String>,
    pub review_reduced: bool,
    pub review_motion: bool,
}
impl StartupConfig {
    pub fn from_env() -> Self {
        Self {
            practice: crate::environment::flag("LASTLIGHT_PRACTICE"),
            scenario: crate::environment::string("LASTLIGHT_REVIEW_SCREEN"),
            review_reduced: crate::environment::flag("LASTLIGHT_REVIEW_REDUCED"),
            review_motion: crate::environment::flag("LASTLIGHT_REVIEW_MOTION"),
        }
    }
}

/// The directory holding one profile's `campaign.json` and `settings.json`.
#[derive(Clone, Debug)]
pub struct ProfilePaths {
    directory: PathBuf,
}
impl ProfilePaths {
    pub fn in_directory(directory: PathBuf) -> Self {
        Self { directory }
    }
    /// Resolves the save location from `LASTLIGHT_SAVE_DIR`/`LOCALAPPDATA` and the config.
    pub fn from_env(config: &StartupConfig) -> Self {
        Self::in_directory(runner::profile_directory(
            settings::save_directory(),
            config.scenario.as_deref(),
            config.practice,
        ))
    }
    pub fn campaign(&self) -> PathBuf {
        self.directory.join("campaign.json")
    }
    pub fn settings(&self) -> PathBuf {
        self.directory.join("settings.json")
    }
}

impl LoadedProfile {
    /// Reads the campaign and settings. Unreadable saves degrade to a fresh
    /// in-memory game plus a warning, so loading itself cannot fail.
    pub fn load(paths: &ProfilePaths) -> Self {
        runner::load_profile(paths.directory.clone())
    }
}
