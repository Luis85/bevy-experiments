//! fl_content: data-driven content.
//!
//! Objects (units, later buildings, …) are RON files holding an open map of named *def
//! components* that plugins register in a [`DefRegistry`]. This crate parses them with
//! registry-driven, type-erased deserialization and resolves `inherits` chains.
//!
//! [`load_dirs`] loads whole mods: manifests and load order, objects with overrides,
//! fixed-schema [`ContentKind`]s (factions, maps, terrain types, scenarios, visuals, …),
//! with a map's PNG layers decoded into [`MapLayers`] (the only PNG reader), `rules.ron`
//! [`RulesSection`]s and credits. It then runs the link pass and computes the rules hash,
//! producing a [`ContentDb`].

#![deny(clippy::float_arithmetic)]

mod builtin;
mod db;
mod def;
mod error;
mod hash;
mod inherit;
mod kinds;
mod layers;
mod link;
mod mods;
mod object;
mod registry;
mod rules;
mod terrain;
mod tree;

pub use builtin::{
    Faction, MapDef, Placeholder, ScenarioDef, ScenarioPlayer, ScenarioUnit, VisualDef,
};
pub use db::{ContentDb, Origin};
pub use def::{DefComponent, SpawnCtx};
pub use error::{ContentError, ContentErrors};
pub use hash::RULES_HASH_FORMAT;
pub use inherit::{Resolved, ResolvedObject, resolve_objects};
pub use kinds::{ContentKind, KindTable, RulesSection};
pub use layers::{MapLayers, MapTerrain, decode_layers, write_layers};
pub use link::LinkCtx;
pub use mods::{ALLOWED_LICENSES, CreditEntry, ModManifest, load_dirs};
pub use object::{ObjectDef, parse_object};
pub use registry::DefRegistry;
pub use terrain::{MoveClass, TerrainDef};
pub use tree::{
    ContentTree, Node, PathSeg, Span, TreeDeserializer, TreeError, from_tree, parse_ron, path_key,
    split_path_key, to_ron,
};
