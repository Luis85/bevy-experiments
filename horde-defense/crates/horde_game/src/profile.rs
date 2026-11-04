//! The active profile: where its campaign and settings live, and the startup
//! read that loads them. Construction of the frontend itself stays pure.
use crate::{launch::LaunchConfig, settings::Settings};
use bevy::prelude::Resource;
use horde_core::sim::Game;
use std::path::PathBuf;

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
        settings: Settings::load(&settings_path),
        warning,
    }
}
