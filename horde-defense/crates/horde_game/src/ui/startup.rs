//! Startup inputs for `Frontend`, split so construction itself stays pure:
//! process configuration lives in `LaunchConfig` (parsed once in `main`),
//! filesystem reads in `LoadedProfile::load`, and `Frontend::new` touches neither.
use crate::{
    launch::LaunchConfig,
    runner::{self, LoadedProfile},
};
use std::path::PathBuf;

/// The directory holding one profile's `campaign.json` and `settings.json`.
#[derive(Clone, Debug)]
pub struct ProfilePaths {
    directory: PathBuf,
}
impl ProfilePaths {
    pub fn in_directory(directory: PathBuf) -> Self {
        Self { directory }
    }
    /// The save location for this launch: the configured save directory, or a
    /// practice/review profile beneath or beside it.
    pub fn from_config(config: &LaunchConfig) -> Self {
        Self::in_directory(runner::profile_directory(
            config.save_directory.clone(),
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
