//! Hermetic `Frontend` construction for tests: a default `LaunchConfig` (no
//! process environment), no real save directory, no filesystem reads.
use super::{Frontend, ProfilePaths};
use crate::launch::LaunchConfig;
use crate::{runner::LoadedProfile, settings::Settings};
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
pub(super) fn test_frontend() -> Frontend {
    let paths = ProfilePaths::in_directory(unique_temp_dir());
    let profile = LoadedProfile {
        game: Game::default(),
        settings: Settings::default(),
        warning: String::new(),
    };
    Frontend::new(profile, &LaunchConfig::default(), &paths)
}

#[test]
fn test_frontend_saves_only_under_the_temp_directory() {
    let f = test_frontend();
    let temp = std::env::temp_dir();
    assert!(f.save_path.starts_with(&temp), "{:?}", f.save_path);
    assert!(f.settings_path.starts_with(&temp), "{:?}", f.settings_path);
    assert!(!f.practice_profile);
    assert!(!f.profile_exists);
}
