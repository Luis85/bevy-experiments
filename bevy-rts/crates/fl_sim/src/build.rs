//! Assembling the sim: the registry for loading content, the sim app itself and its
//! [`MatchHeader`] (ADR-0001, ADR-0004, ADR-0006, ADR-0007).

use std::path::PathBuf;
use std::sync::Arc;

use bevy_app::App;
use bevy_ecs::resource::Resource;
use fl_content::{ContentDb, ContentErrors, DefRegistry, load_dirs};
use fl_protocol::{FORMAT_VERSION, MatchHeader};

use crate::builder::SimBuilder;
use crate::classify::{ClassifiedUpTo, check_classification};
use crate::feature::{FeatureError, SimFeature, sort_features};
use crate::hash::{HashTables, NameKeys, StateHashRules};
use crate::nav::FlowCache;
use crate::observe::{Journal, add_phase_markers};
use crate::rng::MatchSeed;
use crate::schedule::{configure_sim_schedule, initialize_sim_tick};
use crate::step::HashInterval;

/// The version of the sim's rules code; bumped when core sim behaviour changes. Recorded in
/// the [`MatchHeader`]. 2 from M1 (terrain changes the hash stream of existing matches).
pub const SIM_VERSION: u32 = 2;

/// What a match is played with; it goes into the [`MatchHeader`].
#[derive(Clone, Debug, Default)]
pub struct MatchSetup {
    /// The match seed: every random draw derives from it.
    pub seed: u64,
    /// Whether `debug.*` commands are accepted.
    pub cheats_allowed: bool,
}

/// How this process runs the sim; never part of the [`MatchHeader`]. Later tasks add
/// fields: construct it with `..Default::default()`.
#[derive(Clone, Debug, Default)]
pub struct RunConfig {
    /// Overrides the `StateHash.interval_ticks` rule; `Some(0)` turns hashing off.
    pub hash_interval: Option<u32>,
    /// Shuffle the order of unordered systems in `SimTick` with this seed (determinism
    /// checks: the hashes must not change).
    pub shuffle_seed: Option<u64>,
    /// Keep the [`Journal`](crate::Journal); `None` keeps it in debug builds only.
    pub journal: Option<bool>,
    /// Emit the `fl_sim::phase` marker events around every phase (for the profiler).
    pub phase_markers: bool,
    /// Empty the flow-field cache ([`FlowCache`](crate::nav::FlowCache)) at the start of every
    /// tick. A diagnostic: the cache is outcome-neutral (ADR-0015, R84), so the hashes must
    /// not change; only the work counters do.
    pub flow_cache_cold: bool,
}

/// The loaded content, shared with the client. External `"core.content"`.
#[derive(Resource, Clone)]
pub struct Content(pub Arc<ContentDb>);

/// The header of the running match. External `"core.match_header"`.
#[derive(Resource, Clone, Debug)]
pub struct MatchHeaderRes(pub MatchHeader);

/// Why the sim could not be built.
#[derive(Debug, thiserror::Error)]
pub enum BuildError {
    #[error(transparent)]
    Feature(#[from] FeatureError),
    #[error("content failed to load:\n{0}")]
    Content(#[from] ContentErrors),
    #[error("unclassified sim types (register them with SimBuilder): {0:?}")]
    Unclassified(Vec<String>),
    #[error("the content was loaded with a different feature set (content registries differ)")]
    RegistryMismatch,
}

/// `"<CARGO_PKG_VERSION>+<FL_GIT_REV>"` when `FL_GIT_REV` was set at compile time, else
/// `"dev"`.
pub fn build_id() -> String {
    match option_env!("FL_GIT_REV") {
        Some(rev) => format!("{}+{rev}", env!("CARGO_PKG_VERSION")),
        None => "dev".to_owned(),
    }
}

/// The features in build order, each built through one [`SimBuilder`].
fn assemble(
    features: &[Arc<dyn SimFeature>],
) -> Result<(SimBuilder, Vec<Arc<dyn SimFeature>>), BuildError> {
    let sorted = sort_features(features)?;
    let mut builder = SimBuilder::new();
    for feature in &sorted {
        builder.build_feature(feature.as_ref());
    }
    Ok((builder, sorted))
}

/// The content registry that [`CoreFeature`](crate::CoreFeature) plus `features` register,
/// not yet frozen ([`load_dirs`] freezes it).
///
/// # Errors
/// [`BuildError::Feature`] if the features cannot be ordered.
pub fn sim_registry(features: &[Arc<dyn SimFeature>]) -> Result<DefRegistry, BuildError> {
    assemble(features).map(|(builder, _)| builder.into_def_registry())
}

/// Load the mods in `mods` with the registry of [`CoreFeature`](crate::CoreFeature) plus
/// `features`.
///
/// # Errors
/// [`BuildError::Feature`] or [`BuildError::Content`].
pub fn load_content(
    features: &[Arc<dyn SimFeature>],
    mods: &[PathBuf],
) -> Result<Arc<ContentDb>, BuildError> {
    let registry = sim_registry(features)?;
    Ok(Arc::new(load_dirs(registry, mods)?))
}

/// Build the sim app for `content`, ready to step with [`step_tick`](crate::step_tick).
///
/// Sorts the features, builds each through a [`SimBuilder`], freezes every registry, checks
/// that `content` was loaded with the same content registry, and inserts [`Content`], the
/// [`MatchHeaderRes`] (features, mods, rules hash, seed, cheats flag, every string table,
/// [`build_id`], [`SIM_VERSION`]), the [`MatchSeed`] `setup.seed`, the name keys of every
/// string table (for the state hash) and the [`HashInterval`] `run.hash_interval`, or the
/// `StateHash` rule, enables the [`Journal`] per `run.journal` and makes the flow-field cache
/// cold per `run.flow_cache_cold`. Then it configures and
/// builds [`SimTick`](crate::SimTick) (with `run.shuffle_seed`, and the phase markers if
/// `run.phase_markers`), so schedule errors such as ambiguous systems surface here, and in
/// debug builds checks that every sim type the app registered is classified.
///
/// # Errors
/// [`BuildError::Feature`], [`BuildError::RegistryMismatch`] or [`BuildError::Unclassified`].
///
/// # Panics
/// If the `SimTick` schedule does not build.
pub fn build_sim_app(
    features: &[Arc<dyn SimFeature>],
    content: Arc<ContentDb>,
    setup: MatchSetup,
    run: RunConfig,
) -> Result<App, BuildError> {
    let (builder, sorted) = assemble(features)?;
    let (mut app, registry_tables) = builder.finish();
    let ours = app.world().resource::<DefRegistry>().fingerprint();
    if ours != content.registry().fingerprint() {
        return Err(BuildError::RegistryMismatch);
    }

    let mut string_tables = content.string_tables();
    string_tables.tables.extend(registry_tables.tables);
    let header = MatchHeader {
        format_version: FORMAT_VERSION,
        build_id: build_id(),
        sim_version: SIM_VERSION,
        features: sorted
            .iter()
            .map(|f| (f.id().to_owned(), f.version()))
            .collect(),
        mods: content
            .mods()
            .iter()
            .map(|m| (m.id.clone(), m.version.clone()))
            .collect(),
        rules_hash: content.rules_hash(),
        cheats_allowed: setup.cheats_allowed,
        seed: setup.seed,
        string_tables,
    };

    let interval = run
        .hash_interval
        .unwrap_or_else(|| content.rules::<StateHashRules>().interval_ticks);
    let name_keys = NameKeys::new(&header.string_tables);
    app.insert_resource(MatchSeed(header.seed))
        .insert_resource(HashInterval(interval))
        .insert_resource(HashTables(Arc::new(name_keys)))
        .insert_resource(Content(content))
        .insert_resource(MatchHeaderRes(header));
    app.world_mut()
        .resource_mut::<Journal>()
        .set_enabled(run.journal.unwrap_or(cfg!(debug_assertions)));
    app.world_mut()
        .insert_resource(FlowCache::new(0, 0, run.flow_cache_cold));

    configure_sim_schedule(&mut app, run.shuffle_seed);
    if run.phase_markers {
        add_phase_markers(&mut app);
    }
    initialize_sim_tick(app.world_mut());
    // After the schedule is built, so types that only appear in system queries count too.
    if cfg!(debug_assertions) {
        check_classification(app.world()).map_err(BuildError::Unclassified)?;
        let registered = app.world().components().len();
        app.world_mut().resource_mut::<ClassifiedUpTo>().0 = registered;
    }
    Ok(app)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    #[test]
    fn build_id_names_the_git_revision_in_a_checkout() {
        let checkout = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../.git")
            .exists();
        match option_env!("FL_GIT_REV") {
            Some(rev) => {
                assert_eq!(build_id(), format!("{}+{rev}", env!("CARGO_PKG_VERSION")));
                let hash = rev.strip_suffix("-dirty").unwrap_or(rev);
                assert!(
                    hash.len() >= 7 && hash.chars().all(|c| c.is_ascii_hexdigit()),
                    "{rev}"
                );
            }
            None => {
                assert!(!checkout, "a build in a git checkout sets FL_GIT_REV");
                assert_eq!(build_id(), "dev");
            }
        }
    }
}
