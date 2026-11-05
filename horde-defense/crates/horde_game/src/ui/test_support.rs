//! Hermetic `FrontendParts` for tests: a default `LaunchConfig` (never the
//! process environment), no real save directory, no filesystem reads. The
//! helpers below stand in for the App's schedules: the runner passes, the
//! notice system and the state transition (with its `OnEnter` handler).
use super::FrontendParts;
use crate::{
    launch::LaunchConfig,
    profile::{LoadedProfile, ProfilePaths},
    runner,
    settings::Settings,
};
use horde_core::sim::Game;
use std::sync::atomic::{AtomicU64, Ordering};

/// A fresh temp directory path (not created) unique to this call.
fn unique_temp_dir() -> std::path::PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    std::env::temp_dir().join(format!(
        "lastlight-ui-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ))
}

/// Default game and settings, persisted paths under a private temp directory.
pub(super) fn test_frontend() -> FrontendParts {
    let paths = ProfilePaths::in_directory(unique_temp_dir());
    let profile = LoadedProfile {
        game: Game::default(),
        settings: Settings::default(),
        warning: String::new(),
        settings_at_risk: false,
    };
    FrontendParts::new(profile, &LaunchConfig::default(), &paths)
}

impl FrontendParts {
    fn after_runner(&mut self) {
        self.apply_notices();
        self.apply_transitions();
    }
    /// `RuntimeSet::Apply` plus waiting for every save to commit.
    pub(super) fn flush_for_test(&mut self) -> bool {
        let quit = runner::flush_for_test(&mut self.runtime_ctx());
        self.after_runner();
        quit
    }
    /// `RuntimeSet::Apply` without waiting for storage.
    pub(super) fn flush(&mut self) -> bool {
        let quit = runner::flush(&mut self.runtime_ctx());
        self.after_runner();
        quit
    }
    pub(super) fn poll_one_save_for_test(&mut self) -> bool {
        let quit = runner::poll_one_save_for_test(&mut self.runtime_ctx());
        self.after_runner();
        quit
    }
    /// One `RuntimeSet::Advance` pass, watching the battle the UI observes.
    pub(super) fn step(&mut self) {
        let observed = self.ctx().observed_battle();
        self.runtime.observe_battle(observed);
        runner::step(&mut self.runtime_ctx());
        self.after_runner();
    }
}

#[test]
fn test_frontend_saves_only_under_the_temp_directory() {
    let f = test_frontend();
    let temp = std::env::temp_dir();
    assert!(
        f.paths.campaign().starts_with(&temp),
        "{:?}",
        f.paths.campaign()
    );
    assert!(
        f.paths.settings().starts_with(&temp),
        "{:?}",
        f.paths.settings()
    );
    assert!(!f.status.practice_profile);
    assert!(!f.status.profile_exists);
    assert_eq!(
        f.launch,
        LaunchConfig::default(),
        "never the process environment"
    );
}
