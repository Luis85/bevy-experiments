//! [`SimFeature`]: the sim half of a feature (spec §5.2, ADR-0001), feature ordering, and the
//! always-present [`CoreFeature`].

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use fl_content::DefRegistry;
use fl_view::SimEvent;

use crate::build::{Content, MatchHeaderRes};
use crate::builder::SimBuilder;
use crate::classify::{Class, ClassifiedUpTo, StateRegistry, reset_transients};
use crate::clock::SimClock;
use crate::counters::SimCounters;
use crate::events::SimEvents;
use crate::hash::HashTables;
use crate::ids::{SimEntity, SimIdAllocator, SimIndex};
use crate::map::MapInfo;
use crate::players::{Players, players_codec};
use crate::rng::MatchSeed;
use crate::schedule::InputSet;
use crate::step::HashInterval;

/// The sim half of a game mechanic, in its own `fl_feat_<name>` crate: def components, rules
/// sections, classified components and resources, commands, RNG streams and systems, all
/// registered through the [`SimBuilder`]. A new mechanic is a new crate; no core edits.
pub trait SimFeature: Send + Sync + 'static {
    /// A unique, stable id such as `"artillery"`; also the namespace of its registered names.
    fn id(&self) -> &'static str;
    /// Bumped whenever the feature's sim behaviour changes; recorded in the `MatchHeader`.
    fn version(&self) -> u32;
    /// The ids of the features this one builds on. They are built first. `"core"` is
    /// always present and may be listed.
    fn deps(&self) -> &'static [&'static str] {
        &[]
    }
    /// Register everything the sim needs. Called once per sim app, and on a scratch builder
    /// when only the content registry is wanted ([`sim_registry`](crate::sim_registry)).
    fn build(&self, sim: &mut SimBuilder);
}

/// Why a feature list cannot be ordered.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FeatureError {
    #[error("feature id '{0}' is used twice")]
    Duplicate(String),
    #[error("feature id '{0}' is reserved")]
    Reserved(String),
    #[error("feature '{feature}' depends on '{dep}', which is not in the feature list")]
    MissingDep { feature: String, dep: String },
    #[error("features {0:?} cannot be ordered (dependency cycle)")]
    Cycle(Vec<String>),
}

/// [`CoreFeature`] then `features`, ordered so every feature comes after its
/// [`deps`](SimFeature::deps): Kahn's algorithm, taking the smallest ready id first.
///
/// # Errors
/// A feature with id `"core"` ([`FeatureError::Reserved`]), two features with one id
/// ([`FeatureError::Duplicate`]), a dependency that is not in the list
/// ([`FeatureError::MissingDep`]), or a cycle ([`FeatureError::Cycle`], listing every feature
/// that could not be ordered, sorted).
pub fn sort_features(
    features: &[Arc<dyn SimFeature>],
) -> Result<Vec<Arc<dyn SimFeature>>, FeatureError> {
    let mut by_id: BTreeMap<&'static str, &Arc<dyn SimFeature>> = BTreeMap::new();
    for feature in features {
        let id = feature.id();
        if id == CoreFeature::ID {
            return Err(FeatureError::Reserved(id.to_owned()));
        }
        if by_id.insert(id, feature).is_some() {
            return Err(FeatureError::Duplicate(id.to_owned()));
        }
    }

    // Unmet dependency count per feature, and who waits on whom; deps on core are met.
    let mut waiting_on: BTreeMap<&'static str, usize> = BTreeMap::new();
    let mut dependents: BTreeMap<&'static str, Vec<&'static str>> = BTreeMap::new();
    for (&id, feature) in &by_id {
        let deps: BTreeSet<&'static str> = feature
            .deps()
            .iter()
            .copied()
            .filter(|&dep| dep != CoreFeature::ID)
            .collect();
        for &dep in &deps {
            if !by_id.contains_key(dep) {
                return Err(FeatureError::MissingDep {
                    feature: id.to_owned(),
                    dep: dep.to_owned(),
                });
            }
            dependents.entry(dep).or_default().push(id);
        }
        waiting_on.insert(id, deps.len());
    }

    let mut ready: BTreeSet<&'static str> = waiting_on
        .iter()
        .filter(|&(_, &n)| n == 0)
        .map(|(&id, _)| id)
        .collect();
    let mut sorted: Vec<Arc<dyn SimFeature>> = vec![Arc::new(CoreFeature)];
    while let Some(id) = ready.pop_first() {
        waiting_on.remove(id);
        sorted.push(Arc::clone(by_id[id]));
        for &dependent in dependents.get(id).into_iter().flatten() {
            let n = waiting_on
                .get_mut(dependent)
                .expect("a dependent is not placed before its dependency");
            *n -= 1;
            if *n == 0 {
                ready.insert(dependent);
            }
        }
    }
    if !waiting_on.is_empty() {
        return Err(FeatureError::Cycle(
            waiting_on.keys().map(|&id| id.to_owned()).collect(),
        ));
    }
    Ok(sorted)
}

/// The core of every sim, always built first (callers never list it): the clock, ids, match
/// seed and RNG streams, state hash, counters, players, map, spatial rules, terrain and
/// navigation (the `"Movement"`, `"Path"` and `"Steering"` rules), events; the unit def
/// components `"Selectable"`, `"Hull"` and `"Mobility"` and their runtime components; the
/// core and `debug.*` commands; the order queue, path requests, wandering and movement; the
/// spawn and despawn funnels; the scale scenario's mix; snapshot publishing.
pub struct CoreFeature;

impl CoreFeature {
    pub const ID: &'static str = "core";
}

impl SimFeature for CoreFeature {
    fn id(&self) -> &'static str {
        CoreFeature::ID
    }

    /// 2 from M1: the terrain (`"core.terrain"`, filled in M0 saves by its `fill_missing`).
    fn version(&self) -> u32 {
        2
    }

    fn build(&self, sim: &mut SimBuilder) {
        sim.app
            .init_resource::<SimClock>()
            .init_resource::<SimIdAllocator>()
            .init_resource::<SimIndex>()
            .init_resource::<Players>()
            .init_resource::<SimCounters>()
            .init_resource::<ClassifiedUpTo>()
            .init_resource::<SimEvents<SimEvent>>();

        // Derived, kept by hooks: `SimEntity` re-creates itself on load, and its hooks rebuild
        // `SimIndex`. Core's one rebuild (R74), `rebuild_core`, makes the spatial index and the
        // navigation data.
        sim.register_component::<SimEntity>("core.sim_entity", Class::Derived)
            .register_resource::<SimClock>("core.clock", Class::hashed(1))
            .register_resource::<SimIdAllocator>("core.sim_id_allocator", Class::hashed(1))
            .register_resource::<Players>("core.players", Class::Hashed(players_codec()))
            .register_resource::<MapInfo>("core.map", Class::hashed(1))
            .register_resource::<MatchSeed>("core.match_seed", Class::hashed(1))
            .register_resource::<SimIndex>("core.sim_index", Class::Derived)
            .register_resource::<Content>("core.content", Class::External)
            .register_resource::<MatchHeaderRes>("core.match_header", Class::External)
            .register_resource::<DefRegistry>("core.def_registry", Class::External)
            .register_resource::<StateRegistry>("core.state_registry", Class::External)
            .register_resource::<HashTables>("core.hash_tables", Class::External)
            .register_resource::<HashInterval>("core.hash_interval", Class::External)
            .register_resource::<SimEvents<SimEvent>>(
                "core.sim_events",
                Class::TransientReset(SimEvents::clear),
            )
            .register_resource::<SimCounters>(
                "core.counters",
                Class::TransientReset(SimCounters::clear),
            )
            .register_resource::<ClassifiedUpTo>(
                "core.classified_up_to",
                Class::TransientKept("debug bookkeeping of step_tick's classification check"),
            )
            .add_systems(InputSet::Housekeeping, reset_transients)
            .register_rebuild(crate::nav::rebuild_core);
        // The scale scenario's placement draws come from its own seed, in setup code
        // (`ScaleScenario::populate`, `Rolls::new`), not from a sim system: the name is
        // registered to keep it reserved.
        let _placement = sim.register_stream("setup.scale");

        crate::hash::build(sim);
        crate::spatial::build(sim);
        crate::nav::build(sim);
        crate::units::build(sim);
        crate::commands::build(sim);
        crate::orders::build(sim);
        crate::path::build(sim);
        crate::reload::build(sim);
        crate::debug_cmds::build(sim);
        crate::spawn::build(sim);
        crate::movement::build(sim);
        crate::scale::build(sim);
        crate::publish::build(sim);
    }
}
