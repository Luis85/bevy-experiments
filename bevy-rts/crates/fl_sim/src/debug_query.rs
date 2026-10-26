//! Named debug queries (spec §5.3, ADR-0015): read-only questions about sim state that the
//! snapshot doesn't carry, answered between ticks through `SimQuery::Debug`.
//!
//! Features register them with
//! [`SimBuilder::register_debug_query`](crate::SimBuilder::register_debug_query). A query reads
//! the `World` and answers JSON, with `Fx` as its raw `i64` (`Fx::raw`), so no float enters the
//! sim. Queries are observe-only: sim logic never runs or reads them, and they are never
//! hashed.
//!
//! A query never panics: bad or missing args, an unknown id or a world without the state it
//! reads answer `Err` with a message. The threaded runner answers queries on the sim thread,
//! so a panicking query would take the match down with it.

use bevy_ecs::resource::Resource;
use bevy_ecs::world::World;
use serde_json::{Value, json};

use crate::registry::StableRegistry;

/// A debug query: reads `world` with the caller's `args` and answers JSON, or an error message.
/// It must not panic: bad args (and a world without what it reads) are an `Err`.
pub type DebugQueryFn = fn(&World, &Value) -> Result<Value, String>;

/// The name of the built-in query that lists every registered name.
pub(crate) const LIST_QUERIES: &str = "core.debug_queries";

/// Every registered debug query, by name; frozen when the sim is built. External
/// `"core.debug_queries"`. Its dense ids never leave the sim: it is not exported to the
/// string tables.
#[derive(Resource)]
pub struct DebugQueries {
    registry: StableRegistry<DebugQueryFn>,
}

impl DebugQueries {
    /// # Panics
    /// If `registry` is not frozen.
    pub(crate) fn new(registry: StableRegistry<DebugQueryFn>) -> Self {
        assert!(
            registry.is_frozen(),
            "the debug query registry must be frozen"
        );
        DebugQueries { registry }
    }

    /// Every registered name, sorted.
    pub fn names(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.registry.iter().map(|(_, name, _)| name)
    }
}

/// A new registry holding only the built-in [`LIST_QUERIES`], registered by core.
pub(crate) fn registry() -> StableRegistry<DebugQueryFn> {
    let mut registry: StableRegistry<DebugQueryFn> = StableRegistry::new("debug_queries");
    registry.register(LIST_QUERIES, crate::CoreFeature::ID, list_queries);
    registry
}

/// `core.debug_queries`: `{"queries": [every registered name, sorted]}`.
fn list_queries(world: &World, _args: &Value) -> Result<Value, String> {
    let names: Vec<&str> = world.resource::<DebugQueries>().names().collect();
    Ok(json!({ "queries": names }))
}

/// Answer the debug query `name` with `args` from `world`, between ticks.
///
/// # Errors
/// `"unknown debug query '<name>'"` when no query has that name; otherwise the query's own
/// message.
pub fn debug_query(world: &World, name: &str, args: &Value) -> Result<Value, String> {
    let query = world
        .get_resource::<DebugQueries>()
        .and_then(|queries| queries.registry.get(name).copied())
        .ok_or_else(|| format!("unknown debug query '{name}'"))?;
    query(world, args)
}
