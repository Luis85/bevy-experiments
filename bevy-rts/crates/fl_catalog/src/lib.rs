//! fl_catalog: the base mod's location and the list of game features.
//!
//! Headless: the game and the tools take their content directory and their sim features
//! from here.

use std::path::PathBuf;
use std::sync::Arc;

use fl_sim::SimFeature;

/// Every game feature; the sim sorts them by dependency.
///
/// Empty in M0 because the core feature is implicit. Each feature crate adds one line here.
#[must_use]
pub fn features() -> Vec<Arc<dyn SimFeature>> {
    Vec::new()
}

/// The base mod's directory, `assets/base` at the repository root, canonicalized.
///
/// # Panics
/// If the directory does not exist.
#[must_use]
pub fn base_mod_dir() -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/base");
    dir.canonicalize()
        .unwrap_or_else(|err| panic!("base mod dir {} is missing: {err}", dir.display()))
}
