//! The def-component registry: the open set of component names, object kinds and sections.
//!
//! Features fill it while the sim app is built; [`load_dirs`](crate::load_dirs) then freezes
//! it, and its sorted name lists become the dense-id [`StringTables`] (ADR-0004).

use std::any::TypeId;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::Arc;

use bevy_ecs::resource::Resource;
use fl_protocol::{StringTables, tables};
use serde::de::DeserializeOwned;
use xxhash_rust::xxh64::xxh64;

use crate::builtin::{Faction, MapDef, ScenarioDef, VisualDef};
use crate::def::DefComponent;
use crate::error::ContentError;
use crate::kinds::{
    ContentKind, ErasedKind, ErasedKindTable, ErasedRules, RulesSection, build_table, parse_kind,
};
use crate::terrain::{MAX_TERRAIN_KINDS, TerrainDef};
use crate::tree::Node;

/// Deserializes one registered def component from a type-erased deserializer.
pub(crate) type DeserializeFn = for<'de> fn(
    &mut dyn erased_serde::Deserializer<'de>,
) -> Result<Arc<dyn DefComponent>, erased_serde::Error>;

/// A registered def component.
pub(crate) struct ComponentEntry {
    pub(crate) deserialize: DeserializeFn,
}

/// Reads one content-kind item from its file's tree.
pub(crate) type KindParseFn = fn(&Node, &Path) -> Result<Box<dyn ErasedKind>, ContentError>;

/// Builds a kind's table from its items sorted by id.
pub(crate) type BuildTableFn = fn(Vec<Box<dyn ErasedKind>>) -> Box<dyn ErasedKindTable>;

/// A registered content kind (`*.<ext>.ron`).
pub(crate) struct KindEntry {
    pub(crate) ext: &'static str,
    pub(crate) in_rules_hash: bool,
    pub(crate) namespaced: bool,
    /// The most items content may load; one more is a load error.
    pub(crate) max_items: Option<usize>,
    pub(crate) parse: KindParseFn,
    pub(crate) build_table: BuildTableFn,
}

/// Deserializes one registered rules section from a type-erased deserializer.
pub(crate) type RulesDeserializeFn =
    for<'de> fn(
        &mut dyn erased_serde::Deserializer<'de>,
    ) -> Result<Box<dyn ErasedRules>, erased_serde::Error>;

/// A registered `rules.ron` section.
pub(crate) struct RulesEntry {
    pub(crate) deserialize: RulesDeserializeFn,
}

/// Every content name plugins have registered. Built once, before content loads, then
/// frozen: every `register_*` after [`freeze`](Self::freeze) panics.
#[derive(Resource, Default)]
pub struct DefRegistry {
    frozen: bool,
    components: BTreeMap<&'static str, ComponentEntry>,
    type_names: BTreeMap<TypeId, &'static str>,
    object_kinds: BTreeSet<&'static str>,
    /// Content kinds by file extension (`*.faction.ron`, …).
    content_kinds: BTreeMap<&'static str, KindEntry>,
    /// `rules.ron` sections by name; every one is required.
    rules_sections: BTreeMap<&'static str, RulesEntry>,
    rules_types: BTreeMap<TypeId, &'static str>,
}

fn deserialize_component<T: DefComponent + DeserializeOwned>(
    deserializer: &mut dyn erased_serde::Deserializer<'_>,
) -> Result<Arc<dyn DefComponent>, erased_serde::Error> {
    let value: T = erased_serde::deserialize(deserializer)?;
    Ok(Arc::new(value))
}

fn deserialize_rules<S: RulesSection>(
    deserializer: &mut dyn erased_serde::Deserializer<'_>,
) -> Result<Box<dyn ErasedRules>, erased_serde::Error> {
    let value: S = erased_serde::deserialize(deserializer)?;
    Ok(Box::new(value))
}

impl DefRegistry {
    /// A registry with the built-in content: object kind `"unit"` and the kinds
    /// [`Faction`], [`MapDef`], [`TerrainDef`] (at most 255: a cell's type is a `u8`, and
    /// `u8::MAX` is open ground), [`ScenarioDef`] and [`VisualDef`].
    pub fn with_builtins() -> Self {
        let mut reg = DefRegistry::default();
        reg.register_object_kind("unit");
        reg.register_kind::<Faction>();
        reg.register_kind::<MapDef>();
        reg.register_kind_capped::<TerrainDef>(Some(MAX_TERRAIN_KINDS));
        reg.register_kind::<ScenarioDef>();
        reg.register_kind::<VisualDef>();
        reg
    }

    /// Freeze the name lists: from now on every `register_*` panics.
    /// [`load_dirs`](crate::load_dirs) freezes the registry it is given.
    pub fn freeze(&mut self) {
        self.frozen = true;
    }

    /// Whether [`freeze`](Self::freeze) has been called.
    pub fn is_frozen(&self) -> bool {
        self.frozen
    }

    fn assert_not_frozen(&self, name: &str) {
        assert!(
            !self.frozen,
            "DefRegistry is frozen: register '{name}' in SimFeature::build"
        );
    }

    /// Register content kind `K`, loaded from `*.<K::EXT>.ron` files.
    ///
    /// # Panics
    /// If the registry is frozen, or `K::EXT` is already registered as a content kind or an
    /// object kind.
    pub fn register_kind<K: ContentKind>(&mut self) {
        self.register_kind_capped::<K>(None);
    }

    /// [`register_kind`](Self::register_kind), loading at most `max_items` items.
    fn register_kind_capped<K: ContentKind>(&mut self, max_items: Option<usize>) {
        let ext = K::EXT;
        self.assert_not_frozen(ext);
        let type_name = std::any::type_name::<K>();
        assert!(
            !self.object_kinds.contains(ext),
            "content kind {type_name}: extension '{ext}' is already an object kind"
        );
        assert!(
            !self.content_kinds.contains_key(ext),
            "content kind extension '{ext}' is already registered (registering {type_name})"
        );
        self.content_kinds.insert(
            ext,
            KindEntry {
                ext,
                in_rules_hash: K::IN_RULES_HASH,
                namespaced: K::NAMESPACED,
                max_items,
                parse: parse_kind::<K>,
                build_table: build_table::<K>,
            },
        );
    }

    /// Register rules section `S` under `name` (`Rules({ "<name>": (…) })`). Every registered
    /// section must be present in some mod's `rules.ron`.
    ///
    /// # Panics
    /// If the registry is frozen, or `name` or the type `S` is already registered.
    pub fn register_rules_section<S: RulesSection>(&mut self, name: &'static str) {
        self.assert_not_frozen(name);
        let type_name = std::any::type_name::<S>();
        assert!(
            !self.rules_sections.contains_key(name),
            "rules section '{name}' is already registered (registering {type_name})"
        );
        if let Some(existing) = self.rules_types.get(&TypeId::of::<S>()) {
            panic!("rules section type {type_name} is already registered as '{existing}'");
        }
        self.rules_sections.insert(
            name,
            RulesEntry {
                deserialize: deserialize_rules::<S>,
            },
        );
        self.rules_types.insert(TypeId::of::<S>(), name);
    }

    /// Register def component `T` under `name` (the key used in an object's `components` map).
    ///
    /// # Panics
    /// If the registry is frozen, or `name` or the type `T` is already registered.
    pub fn register_component<T: DefComponent + DeserializeOwned>(&mut self, name: &'static str) {
        self.assert_not_frozen(name);
        let type_id = TypeId::of::<T>();
        let type_name = std::any::type_name::<T>();
        assert!(
            !self.components.contains_key(name),
            "def component name '{name}' is already registered (registering {type_name})"
        );
        if let Some(existing) = self.type_names.get(&type_id) {
            panic!("def component type {type_name} is already registered as '{existing}'");
        }
        self.components.insert(
            name,
            ComponentEntry {
                deserialize: deserialize_component::<T>,
            },
        );
        self.type_names.insert(type_id, name);
    }

    /// Register an object kind by file extension: `"unit"` loads `*.unit.ron` files.
    ///
    /// # Panics
    /// If the registry is frozen or the kind is already registered.
    pub fn register_object_kind(&mut self, ext: &'static str) {
        self.assert_not_frozen(ext);
        assert!(
            !self.content_kinds.contains_key(ext),
            "object kind '{ext}' is already a content kind"
        );
        assert!(
            self.object_kinds.insert(ext),
            "object kind '{ext}' is already registered"
        );
    }

    /// The name `T` was registered under, if any.
    pub fn component_name<T: DefComponent>(&self) -> Option<&'static str> {
        self.type_names.get(&TypeId::of::<T>()).copied()
    }

    /// The registered object kind matching `ext`, as the registry's `'static` name.
    pub fn object_kind(&self, ext: &str) -> Option<&'static str> {
        self.object_kinds.get(ext).copied()
    }

    /// Registered def component names, sorted.
    pub(crate) fn component_names(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.components.keys().copied()
    }

    pub(crate) fn component(&self, name: &str) -> Option<&ComponentEntry> {
        self.components.get(name)
    }

    /// Registered object kinds, sorted.
    pub(crate) fn object_kinds(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.object_kinds.iter().copied()
    }

    /// The registered content kind for `ext`.
    pub(crate) fn kind_entry(&self, ext: &str) -> Option<&KindEntry> {
        self.content_kinds.get(ext)
    }

    /// Registered content kinds, sorted by extension.
    pub(crate) fn kind_entries(&self) -> impl Iterator<Item = &KindEntry> + '_ {
        self.content_kinds.values()
    }

    /// The registered rules section `name`, with its `'static` name.
    pub(crate) fn rules_entry(&self, name: &str) -> Option<(&'static str, &RulesEntry)> {
        self.rules_sections
            .get_key_value(name)
            .map(|(k, v)| (*k, v))
    }

    /// Registered rules section names, sorted.
    pub(crate) fn rules_section_names(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.rules_sections.keys().copied()
    }

    /// The name rules section type `S` was registered under, if any.
    pub(crate) fn rules_section_name<S: RulesSection>(&self) -> Option<&'static str> {
        self.rules_types.get(&TypeId::of::<S>()).copied()
    }

    /// The registered names as string tables: [`tables::DEF_COMPONENTS`],
    /// [`tables::OBJECT_KINDS`], [`tables::CONTENT_KINDS`] and [`tables::RULES_SECTIONS`],
    /// each sorted, so a name's position is its dense id.
    pub fn string_tables(&self) -> StringTables {
        fn owned<'a>(names: impl Iterator<Item = &'a &'static str>) -> Vec<String> {
            names.map(|name| (*name).to_owned()).collect()
        }
        let mut out = StringTables::default();
        out.insert(tables::DEF_COMPONENTS, owned(self.components.keys()));
        out.insert(tables::OBJECT_KINDS, owned(self.object_kinds.iter()));
        out.insert(tables::CONTENT_KINDS, owned(self.content_kinds.keys()));
        out.insert(tables::RULES_SECTIONS, owned(self.rules_sections.keys()));
        out
    }

    /// xxh64 over the postcard bytes of [`string_tables`](Self::string_tables).
    /// Two builds with the same registered features have the same fingerprint.
    pub fn fingerprint(&self) -> u64 {
        let bytes = postcard::to_allocvec(&self.string_tables())
            .expect("string tables serialize to postcard");
        xxh64(&bytes, 0)
    }
}
