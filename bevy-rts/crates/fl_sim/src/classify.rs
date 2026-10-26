//! State classification (ADR-0006): every sim component and resource is exactly one of
//! [`Class::Hashed`], [`Class::Derived`], [`Class::Transient`] or [`Class::External`].
//!
//! Features classify their types with
//! [`SimBuilder::register_component`](crate::SimBuilder::register_component) and
//! [`SimBuilder::register_resource`](crate::SimBuilder::register_resource). The names land in
//! the `"sim_components"` and `"sim_resources"` string tables, and
//! [`build_sim_app`](crate::build_sim_app) fails on any sim type nobody classified
//! ([`check_classification`]). In debug builds [`step_tick`](crate::step_tick) checks again
//! after every tick that registered new types (a type a spawn or command first inserts), and
//! that every entity with a classified component has a [`SimEntity`].
//!
//! Transient state is per-tick, and the framework makes it so (R72): at the start of every
//! tick [`InputSet::Housekeeping`](crate::InputSet::Housekeeping) removes every Transient
//! component and resets every Transient resource ([`Class::transient`],
//! [`Class::TransientReset`]) unless it opted out with a named reason
//! ([`Class::TransientKept`]); [`step_tick`](crate::step_tick) clears every message buffer at
//! the end of the tick.
//!
//! Every [`Codec`] traces its type's serde format; [`codec_formats`] lists them all, so a
//! snapshot pins every save format (ADR-0011).

use std::any::TypeId;
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::sync::Arc;

use bevy_app::App;
use bevy_ecs::component::{Component, ComponentId};
use bevy_ecs::entity::Entity;
use bevy_ecs::hierarchy::{ChildOf, Children};
use bevy_ecs::message::{Message, Messages};
use bevy_ecs::observer::Observer;
use bevy_ecs::query::With;
use bevy_ecs::resource::Resource;
use bevy_ecs::world::{Mut, World};
use fl_protocol::tables;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use serde_reflection::{Format, Registry, Tracer, TracerConfig};

use crate::hash::{SimHash, StateHasher};
use crate::ids::SimEntity;
use crate::registry::StableRegistry;
use crate::save::{LoadCtx, SaveCtx};

/// How the sim treats one component or resource type.
pub enum Class<T> {
    /// Authoritative state: hashed and saved with a versioned codec.
    Hashed(Codec<T>),
    /// Rebuilt from hashed state on load (indices, caches); never hashed or saved. The
    /// feature's one rebuild ([`SimBuilder::register_rebuild`](crate::SimBuilder::register_rebuild))
    /// rebuilds all of its `Derived` state; state that component hooks maintain needs none.
    Derived,
    /// Components only: per-tick data, removed from every entity at the start of every tick
    /// (in [`InputSet::Housekeeping`](crate::InputSet::Housekeeping)); never hashed or saved.
    /// A Transient resource is [`TransientReset`](Self::TransientReset) (or
    /// [`Class::transient`]) or [`TransientKept`](Self::TransientKept).
    Transient,
    /// Resources only: a per-tick buffer, reset with this function at the start of every tick
    /// (in [`InputSet::Housekeeping`](crate::InputSet::Housekeeping)); never hashed or saved.
    /// [`Class::transient`] resets to `Default`.
    TransientReset(fn(&mut T)),
    /// Resources only: a per-tick buffer the framework does not reset, for the named reason
    /// (its owner empties it at another point of the tick, such as the journal that
    /// [`step_tick`](crate::step_tick) drains); never hashed or saved.
    TransientKept(&'static str),
    /// Resources only: fixed at build from the content and the `MatchHeader`.
    External,
}

impl<T: Default> Class<T> {
    /// [`Class::TransientReset`] to `T::default()`: a resource that holds only the current
    /// tick's data.
    pub fn transient() -> Class<T> {
        Class::TransientReset(reset_to_default::<T>)
    }
}

/// Set `value` back to `T::default()`.
fn reset_to_default<T: Default>(value: &mut T) {
    *value = T::default();
}

impl<T: SimHash + Serialize + DeserializeOwned + Send + Sync + 'static> Class<T> {
    /// [`Class::Hashed`] with the [`SimHash`], postcard and JSON codec of `T`
    /// ([`Codec::new`]).
    pub fn hashed(version: u32) -> Class<T> {
        Class::Hashed(Codec::new(version))
    }
}

/// The versioned codec of a [`Class::Hashed`] type: its state hash, its save encoding at
/// [`version`](Self::version), the decoders of older versions (typed migrations), a JSON
/// view and the trace of its serde format.
pub struct Codec<T> {
    /// The save-format version of `T`; bump it (and add a migration) when `T` changes.
    pub version: u32,
    /// Feeds the state hash.
    pub hash: fn(&T, &mut StateHasher),
    /// The save encoding of `T` at [`version`](Self::version).
    pub encode: fn(&T, &SaveCtx<'_>) -> Vec<u8>,
    /// Decodes bytes saved at the given version: always the current one for [`Codec::new`],
    /// any version up to the current one without a migration for [`Codec::custom`].
    pub decode: fn(u32, &[u8], &LoadCtx<'_>) -> Result<T, String>,
    /// A JSON view for inspection (observe-only).
    pub dump: fn(&T) -> Value,
    /// Traces the serde-reflection format of the save encoding into a tracer and returns its
    /// root format. [`Codec::new`] traces `T` itself (as postcard sees it: not
    /// human-readable); [`Codec::custom`] takes the trace of the bytes its `encode` writes.
    pub trace: TraceFn,
    /// Enums nested in `T`, traced before [`trace`](Self::trace), from
    /// [`trace_enum`](Self::trace_enum).
    nested_enums: Vec<TraceFn>,
    /// Decoders of older versions, from [`migrate_from`](Self::migrate_from).
    migrations: Vec<(u32, Migration<T>)>,
    /// Whether [`decode`](Self::decode) also reads older versions ([`Codec::custom`]).
    decodes_old: bool,
    /// Fills the state when an older feature version's save lacks it, from
    /// [`fill_missing`](Self::fill_missing).
    fill: Option<FillFn>,
}

/// Decodes the bytes of one older version into the current `T`.
type Migration<T> = fn(&[u8]) -> Result<T, String>;

/// Fills state that a save of an older feature version lacks ([`Codec::fill_missing`]).
pub type FillFn = fn(&mut World) -> Result<(), String>;

/// Traces one type into a serde-reflection tracer and returns its format.
pub type TraceFn = fn(&mut Tracer) -> serde_reflection::Result<Format>;

impl<T> Clone for Codec<T> {
    fn clone(&self) -> Self {
        Codec {
            version: self.version,
            hash: self.hash,
            encode: self.encode,
            decode: self.decode,
            dump: self.dump,
            trace: self.trace,
            nested_enums: self.nested_enums.clone(),
            migrations: self.migrations.clone(),
            decodes_old: self.decodes_old,
            fill: self.fill,
        }
    }
}

impl<T: SimHash + Serialize + DeserializeOwned + Send + Sync + 'static> Codec<T> {
    /// `T`'s [`SimHash`], its postcard bytes as the save encoding at `version`, and its JSON
    /// view.
    pub fn new(version: u32) -> Self {
        Codec {
            version,
            hash: T::sim_hash,
            encode: encode_postcard::<T>,
            decode: decode_postcard::<T>,
            dump: dump_json::<T>,
            trace: trace_type::<T>,
            nested_enums: Vec::new(),
            migrations: Vec::new(),
            decodes_old: false,
            fill: None,
        }
    }

    /// Like [`Codec::new`], with a hand-written save encoding: for state that holds dense
    /// registry ids, which `decode` maps to the current ids with [`LoadCtx::remap`]. `decode`
    /// gets the saved version and is called for every version up to `version` that no
    /// [`migrate_from`](Self::migrate_from) covers. `trace` traces the format of the bytes
    /// `encode` writes (such as [`trace_type`]`::<u16>` for a bare dense id), so the format
    /// snapshot pins the encoding actually saved, not `T`.
    pub fn custom(
        version: u32,
        encode: fn(&T, &SaveCtx<'_>) -> Vec<u8>,
        decode: fn(u32, &[u8], &LoadCtx<'_>) -> Result<T, String>,
        trace: TraceFn,
    ) -> Self {
        Codec {
            encode,
            decode,
            trace,
            decodes_old: true,
            ..Codec::new(version)
        }
    }
}

impl<T> Codec<T> {
    /// Read bytes saved at `version` (older than [`version`](Self::version)) as postcard
    /// `Old`, then convert them with `Old::into`. A typed migration: keep `Old` (in a
    /// `mod v<N>`) for as long as such saves must load.
    ///
    /// # Panics
    /// If `version` is not older than the codec's version, or already has a migration.
    #[must_use]
    pub fn migrate_from<Old: DeserializeOwned + Into<T>>(mut self, version: u32) -> Self {
        assert!(
            version < self.version,
            "a migration reads an older version: {version} is not below {}",
            self.version
        );
        assert!(
            self.migrations.iter().all(|(from, _)| *from != version),
            "version {version} already has a migration"
        );
        self.migrations.push((version, migrate::<Old, T>));
        self
    }

    /// Fill the state with `fill` when a save lacks this state name and was made by an older
    /// version of the owning feature than this build's (R78, ADR-0006 M1 refinement): state a
    /// new feature version added. [`load_world`](crate::load_world) runs every fill after
    /// it restored every saved value and before the `Derived` rebuilds, in feature load order,
    /// then by state name; a fill's `Err` is [`LoadError::Codec`](crate::LoadError::Codec).
    /// A component's name counts as lacking when no saved entity has it.
    ///
    /// Without a fill, or for a save of the same feature version, a missing name loads as
    /// before: a missing `Hashed` resource is removed, and a component stays absent.
    #[must_use]
    pub fn fill_missing(mut self, fill: FillFn) -> Self {
        self.fill = Some(fill);
        self
    }

    /// Also trace enum `E`, nested in `T`. A trace of `T` reaches every variant of `T` itself
    /// but only one of each enum nested in it, so [`codec_formats`] fails on a nested enum
    /// until its codec traces it.
    #[must_use]
    pub fn trace_enum<E: DeserializeOwned>(mut self) -> Self {
        self.nested_enums.push(trace_type::<E>);
        self
    }

    /// The older versions this codec decodes, for the header of its [`codec_formats`]
    /// block: `""` for none, `" reads older"` for a [`Codec::custom`] (its `decode` reads
    /// every older version), else `" reads <v>, <v>"`, its migrations in version order.
    fn reads(&self) -> String {
        if self.decodes_old {
            return " reads older".to_owned();
        }
        let mut versions: Vec<u32> = self.migrations.iter().map(|&(from, _)| from).collect();
        if versions.is_empty() {
            return String::new();
        }
        versions.sort_unstable();
        let list: Vec<String> = versions.iter().map(u32::to_string).collect();
        format!(" reads {}", list.join(", "))
    }

    /// The root format of the save encoding and every named type it uses, from
    /// [`trace_enum`](Self::trace_enum)'s enums, then [`trace`](Self::trace).
    fn formats(&self) -> serde_reflection::Result<(Format, Registry)> {
        let mut tracer = Tracer::new(TracerConfig::default());
        for trace in &self.nested_enums {
            trace(&mut tracer)?;
        }
        let root = (self.trace)(&mut tracer)?;
        Ok((root, tracer.registry()?))
    }

    /// Decode bytes saved at `version`: a migration if one reads `version`, else
    /// [`decode`](Self::decode) when it reads `version`.
    pub(crate) fn decode_saved(
        &self,
        version: u32,
        bytes: &[u8],
        ctx: &LoadCtx<'_>,
    ) -> Result<T, CodecError> {
        if let Some((_, migrate)) = self.migrations.iter().find(|(from, _)| *from == version) {
            return migrate(bytes).map_err(CodecError::Codec);
        }
        if version == self.version || (self.decodes_old && version < self.version) {
            return (self.decode)(version, bytes, ctx).map_err(CodecError::Codec);
        }
        Err(CodecError::NoMigration {
            from: version,
            to: self.version,
        })
    }
}

/// Why saved bytes did not decode.
#[derive(Debug)]
pub(crate) enum CodecError {
    /// Nothing reads this version.
    NoMigration { from: u32, to: u32 },
    /// The bytes are not a value of the saved version.
    Codec(String),
}

/// `value`'s postcard bytes.
///
/// # Panics
/// If `T`'s `Serialize` does not support postcard (such as `serialize_any`).
fn encode_postcard<T: Serialize>(value: &T, _ctx: &SaveCtx<'_>) -> Vec<u8> {
    postcard::to_allocvec(value).unwrap_or_else(|err| {
        panic!(
            "{} does not serialize to postcard: {err}",
            std::any::type_name::<T>()
        )
    })
}

/// The current version's postcard bytes.
fn decode_postcard<T: DeserializeOwned>(
    _version: u32,
    bytes: &[u8],
    _ctx: &LoadCtx<'_>,
) -> Result<T, String> {
    from_postcard(bytes)
}

/// `bytes` as postcard `Old`, converted to `T`.
fn migrate<Old: DeserializeOwned + Into<T>, T>(bytes: &[u8]) -> Result<T, String> {
    from_postcard::<Old>(bytes).map(Into::into)
}

/// `bytes` as postcard `T`, all of them: trailing bytes are an error.
pub(crate) fn from_postcard<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, String> {
    match postcard::take_from_bytes::<T>(bytes) {
        Ok((value, [])) => Ok(value),
        Ok((_, rest)) => Err(format!("{} trailing bytes", rest.len())),
        Err(err) => Err(err.to_string()),
    }
}

fn dump_json<T: Serialize>(value: &T) -> Value {
    serde_json::to_value(value)
        .unwrap_or_else(|err| Value::String(format!("<cannot serialize to JSON: {err}>")))
}

/// Traces `T`, every variant if it is an enum: the [`TraceFn`] of `T`'s serde form.
///
/// # Errors
/// If `T` cannot be traced (such as a self-describing type).
pub fn trace_type<T: DeserializeOwned>(tracer: &mut Tracer) -> serde_reflection::Result<Format> {
    tracer.trace_simple_type::<T>().map(|(format, _)| format)
}

/// The serde-reflection format of every `Hashed` component and resource codec of the sim in
/// `app` ([`Codec::trace`], after [`Codec::trace_enum`]), in name order: one block per codec,
/// a `<name>@<version>:` line (`<name>@<version> reads <v>, ...:` with the versions its
/// migrations read, `... reads older:` for a [`Codec::custom`], which reads every older one),
/// then indented, `root: <format>` and one `<Type>: <format>` line per named type it uses.
/// The format guard fails a version bump whose block does not read the committed version.
///
/// # Panics
/// If `app` was not built by [`build_sim_app`](crate::build_sim_app), or a codec cannot be
/// traced: its type is self-describing (such as `serde_json::Value`), or an enum nested in it
/// is not traced with [`Codec::trace_enum`].
pub fn codec_formats(app: &App) -> String {
    let state = app.world().resource::<StateRegistry>();
    let components = state.components.iter().filter_map(|(_, name, entry)| {
        let StateClass::Hashed { version } = entry.class else {
            return None;
        };
        let codec = entry.codec.as_deref()?;
        Some((name, version, codec.reads(), codec.formats()))
    });
    let resources = state.resources.iter().filter_map(|(_, name, entry)| {
        let StateClass::Hashed { version } = entry.class else {
            return None;
        };
        let codec = entry.codec.as_deref()?;
        Some((name, version, codec.reads(), codec.formats()))
    });
    let mut codecs: Vec<_> = components.chain(resources).collect();
    codecs.sort_by_key(|&(name, _, _, _)| name);

    let mut out = String::new();
    for (name, version, reads, formats) in codecs {
        let (root, registry) = formats.unwrap_or_else(|err| {
            panic!(
                "the codec of '{name}' cannot be traced: {err} (trace each nested enum with \
                 `Codec::trace_enum`)"
            )
        });
        writeln!(out, "{name}@{version}{reads}:\n  root: {root:?}").expect("writing to a String");
        for (ty, format) in &registry {
            writeln!(out, "  {ty}: {format:?}").expect("writing to a String");
        }
    }
    out
}

/// A [`Class`] without its type.
#[derive(Clone, Copy, Debug)]
pub(crate) enum StateClass {
    Hashed { version: u32 },
    Derived,
    Transient,
    External,
}

/// A [`Codec`] of a component type, applied to one entity.
pub(crate) trait ComponentCodec: Send + Sync {
    /// If the entity has the component: `key` (the component's name key), then its data;
    /// nothing otherwise, so a component no entity has leaves the hash alone. Returns whether
    /// the entity has it.
    fn hash(&self, world: &World, entity: Entity, key: u64, h: &mut StateHasher) -> bool;
    /// Whether the entity has the component.
    fn contains(&self, world: &World, entity: Entity) -> bool;
    /// The component as JSON, if the entity has it.
    fn dump(&self, world: &World, entity: Entity) -> Option<Value>;
    /// The component's save bytes, if the entity has it.
    fn save(&self, world: &World, entity: Entity, ctx: &SaveCtx<'_>) -> Option<Vec<u8>>;
    /// The traced formats of the codec ([`Codec::trace`]).
    fn formats(&self) -> serde_reflection::Result<(Format, Registry)>;
    /// The older versions the codec reads ([`Codec::reads`]).
    fn reads(&self) -> String;
    /// The codec's [`Codec::fill_missing`], if any.
    fn fill(&self) -> Option<FillFn>;
    /// Decode bytes saved at `version` and insert the component on `entity`.
    fn load(
        &self,
        world: &mut World,
        entity: Entity,
        version: u32,
        bytes: &[u8],
        ctx: &LoadCtx<'_>,
    ) -> Result<(), CodecError>;
}

struct TypedComponentCodec<C>(Codec<C>);

impl<C: Component> ComponentCodec for TypedComponentCodec<C> {
    fn hash(&self, world: &World, entity: Entity, key: u64, h: &mut StateHasher) -> bool {
        let Some(component) = world.get::<C>(entity) else {
            return false;
        };
        h.write_u64(key);
        (self.0.hash)(component, h);
        true
    }

    fn contains(&self, world: &World, entity: Entity) -> bool {
        world.get::<C>(entity).is_some()
    }

    fn formats(&self) -> serde_reflection::Result<(Format, Registry)> {
        self.0.formats()
    }

    fn reads(&self) -> String {
        self.0.reads()
    }

    fn fill(&self) -> Option<FillFn> {
        self.0.fill
    }

    fn dump(&self, world: &World, entity: Entity) -> Option<Value> {
        world.get::<C>(entity).map(self.0.dump)
    }

    fn save(&self, world: &World, entity: Entity, ctx: &SaveCtx<'_>) -> Option<Vec<u8>> {
        world
            .get::<C>(entity)
            .map(|component| (self.0.encode)(component, ctx))
    }

    fn load(
        &self,
        world: &mut World,
        entity: Entity,
        version: u32,
        bytes: &[u8],
        ctx: &LoadCtx<'_>,
    ) -> Result<(), CodecError> {
        let component = self.0.decode_saved(version, bytes, ctx)?;
        world.entity_mut(entity).insert(component);
        Ok(())
    }
}

/// A classified component type.
pub(crate) struct ComponentEntry {
    /// The component's type.
    type_id: TypeId,
    /// Removes the component from every entity: `Some` exactly for [`Class::Transient`].
    remove_all: Option<fn(&mut World)>,
    pub(crate) class: StateClass,
    /// `Some` exactly for [`Class::Hashed`].
    pub(crate) codec: Option<Box<dyn ComponentCodec>>,
    /// The feature that classified the type.
    pub(crate) feature: &'static str,
}

/// A [`Codec`] of a resource type, applied to the world.
pub(crate) trait ResourceCodec: Send + Sync {
    /// If the world has the resource: `key` (the resource's name key), then its data;
    /// nothing otherwise.
    fn hash(&self, world: &World, key: u64, h: &mut StateHasher);
    /// Whether the world has the resource.
    fn contains(&self, world: &World) -> bool;
    /// The resource's save bytes, if the world has it.
    fn save(&self, world: &World, ctx: &SaveCtx<'_>) -> Option<Vec<u8>>;
    /// The traced formats of the codec ([`Codec::trace`]).
    fn formats(&self) -> serde_reflection::Result<(Format, Registry)>;
    /// The older versions the codec reads ([`Codec::reads`]).
    fn reads(&self) -> String;
    /// The codec's [`Codec::fill_missing`], if any.
    fn fill(&self) -> Option<FillFn>;
    /// Decode bytes saved at `version` and insert the resource.
    fn load(
        &self,
        world: &mut World,
        version: u32,
        bytes: &[u8],
        ctx: &LoadCtx<'_>,
    ) -> Result<(), CodecError>;
    /// Remove the resource (a save without it).
    fn remove(&self, world: &mut World);
}

struct TypedResourceCodec<R>(Codec<R>);

impl<R: Resource> ResourceCodec for TypedResourceCodec<R> {
    fn hash(&self, world: &World, key: u64, h: &mut StateHasher) {
        if let Some(resource) = world.get_resource::<R>() {
            h.write_u64(key);
            (self.0.hash)(resource, h);
        }
    }

    fn contains(&self, world: &World) -> bool {
        world.contains_resource::<R>()
    }

    fn formats(&self) -> serde_reflection::Result<(Format, Registry)> {
        self.0.formats()
    }

    fn reads(&self) -> String {
        self.0.reads()
    }

    fn fill(&self) -> Option<FillFn> {
        self.0.fill
    }

    fn save(&self, world: &World, ctx: &SaveCtx<'_>) -> Option<Vec<u8>> {
        world
            .get_resource::<R>()
            .map(|resource| (self.0.encode)(resource, ctx))
    }

    fn load(
        &self,
        world: &mut World,
        version: u32,
        bytes: &[u8],
        ctx: &LoadCtx<'_>,
    ) -> Result<(), CodecError> {
        let resource = self.0.decode_saved(version, bytes, ctx)?;
        world.insert_resource(resource);
        Ok(())
    }

    fn remove(&self, world: &mut World) {
        world.remove_resource::<R>();
    }
}

/// A feature's `Derived` rebuild ([`SimBuilder::register_rebuild`](crate::SimBuilder::register_rebuild)).
type Rebuild = fn(&mut World);

/// Resets a Transient resource ([`Class::TransientReset`]).
type ResetFn = Arc<dyn Fn(&mut World) + Send + Sync>;

/// A classified resource type.
pub(crate) struct ResourceEntry {
    /// `Some` exactly for [`Class::TransientReset`].
    reset: Option<ResetFn>,
    pub(crate) class: StateClass,
    /// `Some` exactly for [`Class::Hashed`].
    pub(crate) codec: Option<Box<dyn ResourceCodec>>,
    /// The feature that classified the type.
    pub(crate) feature: &'static str,
}

/// Every classified component and resource type, by name. Frozen when the sim is built.
#[derive(Resource)]
pub(crate) struct StateRegistry {
    pub(crate) components: StableRegistry<ComponentEntry>,
    pub(crate) resources: StableRegistry<ResourceEntry>,
    /// Every classified type, with its name and the feature that classified it.
    types: BTreeMap<TypeId, (&'static str, &'static str)>,
    /// Clears one message buffer of [`SimBuilder::add_message`](crate::SimBuilder::add_message),
    /// per buffer, in registration order.
    message_clears: Vec<fn(&mut World)>,
    /// What [`reset_transients`] and [`clear_message_buffers`] run, collected when the
    /// registry is frozen (so the per-tick work allocates nothing).
    per_tick: Arc<PerTick>,
    /// Each feature's one `Derived` rebuild, in feature load order (core first).
    rebuilds: Vec<(&'static str, Rebuild)>,
}

/// The per-tick resets of the frozen [`StateRegistry`], in name order.
#[derive(Default)]
struct PerTick {
    /// The reset of every [`Class::TransientReset`] resource.
    resets: Vec<ResetFn>,
    /// The removal of every [`Class::Transient`] component.
    removals: Vec<fn(&mut World)>,
    /// The clear of every message buffer.
    message_clears: Vec<fn(&mut World)>,
}

impl Default for StateRegistry {
    fn default() -> Self {
        StateRegistry {
            components: StableRegistry::new(tables::SIM_COMPONENTS),
            resources: StableRegistry::new(tables::SIM_RESOURCES),
            types: BTreeMap::new(),
            message_clears: Vec::new(),
            per_tick: Arc::default(),
            rebuilds: Vec::new(),
        }
    }
}

impl StateRegistry {
    /// # Panics
    /// If the type is already classified, under any name (naming both features).
    fn claim<T: 'static>(&mut self, name: &'static str, registrant: &'static str) {
        if let Some((existing, by)) = self.types.insert(TypeId::of::<T>(), (name, registrant)) {
            panic!(
                "{} is classified twice: as '{existing}' by '{by}' and as '{name}' by \
                 '{registrant}'",
                std::any::type_name::<T>()
            );
        }
    }

    /// # Panics
    /// If `class` is [`Class::External`] ("External is for resources"),
    /// [`Class::TransientReset`] or [`Class::TransientKept`] (a Transient component is
    /// removed every tick), the type is already classified, `name` is taken, or the registry
    /// is frozen.
    pub(crate) fn register_component<C: Component>(
        &mut self,
        name: &'static str,
        registrant: &'static str,
        class: Class<C>,
    ) {
        let (class, codec): (StateClass, Option<Box<dyn ComponentCodec>>) = match class {
            Class::Hashed(codec) => (
                StateClass::Hashed {
                    version: codec.version,
                },
                Some(Box::new(TypedComponentCodec(codec))),
            ),
            Class::Derived => (StateClass::Derived, None),
            Class::Transient => (StateClass::Transient, None),
            Class::TransientReset(_) | Class::TransientKept(_) => panic!(
                "component '{name}' ({}) cannot be TransientReset or TransientKept: a Transient \
                 component is removed every tick (Class::Transient)",
                std::any::type_name::<C>()
            ),
            Class::External => panic!(
                "component '{name}' ({}) cannot be External: External is for resources",
                std::any::type_name::<C>()
            ),
        };
        self.claim::<C>(name, registrant);
        let remove_all: Option<fn(&mut World)> =
            matches!(class, StateClass::Transient).then_some(remove_all::<C>);
        let entry = ComponentEntry {
            type_id: TypeId::of::<C>(),
            remove_all,
            class,
            codec,
            feature: registrant,
        };
        self.components.register(name, registrant, entry);
    }

    /// # Panics
    /// If `class` is [`Class::Transient`] (a Transient resource is reset or kept, with a
    /// reason), the type is already classified, `name` is taken, or the registry is frozen.
    pub(crate) fn register_resource<R: Resource>(
        &mut self,
        name: &'static str,
        registrant: &'static str,
        class: Class<R>,
    ) {
        let reset: Option<ResetFn> = match &class {
            &Class::TransientReset(reset) => Some(Arc::new(move |world: &mut World| {
                // A scope, not `get_resource_mut`: that needs a mutable `R`, which
                // `register_resource` does not require of every resource.
                world.try_resource_scope(|_, mut resource: Mut<R>| reset(&mut resource));
            })),
            _ => None,
        };
        let (class, codec): (StateClass, Option<Box<dyn ResourceCodec>>) = match class {
            Class::Hashed(codec) => (
                StateClass::Hashed {
                    version: codec.version,
                },
                Some(Box::new(TypedResourceCodec(codec))),
            ),
            Class::Derived => (StateClass::Derived, None),
            Class::Transient => panic!(
                "resource '{name}' ({}) cannot be plain Transient: reset it every tick \
                 (Class::transient() or Class::TransientReset) or say why not \
                 (Class::TransientKept)",
                std::any::type_name::<R>()
            ),
            Class::TransientReset(_) | Class::TransientKept(_) => (StateClass::Transient, None),
            Class::External => (StateClass::External, None),
        };
        self.claim::<R>(name, registrant);
        let entry = ResourceEntry {
            reset,
            class,
            codec,
            feature: registrant,
        };
        self.resources.register(name, registrant, entry);
    }

    /// The `Derived` rebuild of `feature`. Features are built in load order, so the
    /// rebuilds are kept in it.
    ///
    /// # Panics
    /// If `feature` already has one.
    pub(crate) fn register_rebuild(&mut self, feature: &'static str, rebuild: Rebuild) {
        assert!(
            self.rebuilds.iter().all(|&(f, _)| f != feature),
            "feature '{feature}' registers a second Derived rebuild: one rebuild per feature \
             rebuilds all of its Derived state, in the order it needs"
        );
        self.rebuilds.push((feature, rebuild));
    }

    /// Every feature's `Derived` rebuild, in feature load order.
    pub(crate) fn rebuilds(&self) -> Vec<Rebuild> {
        self.rebuilds.iter().map(|&(_, rebuild)| rebuild).collect()
    }

    /// Clear `M`'s buffer at the end of every [`step_tick`](crate::step_tick).
    pub(crate) fn clear_messages_each_tick<M: Message>(&mut self) {
        self.message_clears.push(clear_messages::<M>);
    }

    pub(crate) fn freeze(&mut self) {
        self.components.freeze();
        self.resources.freeze();
        self.per_tick = Arc::new(PerTick {
            resets: self
                .resources
                .iter()
                .filter_map(|(_, _, entry)| entry.reset.clone())
                .collect(),
            removals: self
                .components
                .iter()
                .filter_map(|(_, _, entry)| entry.remove_all)
                .collect(),
            message_clears: self.message_clears.clone(),
        });
    }

    /// Whether `type_id` is classified.
    fn contains(&self, type_id: TypeId) -> bool {
        self.types.contains_key(&type_id)
    }
}

/// Remove component `C` from every entity that has it.
fn remove_all<C: Component>(world: &mut World) {
    let entities: Vec<Entity> = world
        .query_filtered::<Entity, With<C>>()
        .iter(world)
        .collect();
    for entity in entities {
        world.entity_mut(entity).remove::<C>();
    }
}

/// Empty `M`'s buffer, if the world has it.
fn clear_messages<M: Message>(world: &mut World) {
    if let Some(mut messages) = world.get_resource_mut::<Messages<M>>() {
        messages.clear();
    }
}

/// The start of every tick, in the core slot of
/// [`InputSet::Housekeeping`](crate::InputSet::Housekeeping) (R72): reset every Transient
/// resource that has a reset ([`Class::TransientReset`], [`Class::transient`]), then remove
/// every Transient component from every entity, each in name order. Exclusive.
pub(crate) fn reset_transients(world: &mut World) {
    let per_tick = Arc::clone(&world.resource::<StateRegistry>().per_tick);
    for reset in &per_tick.resets {
        reset(world);
    }
    for remove_all in &per_tick.removals {
        remove_all(world);
    }
}

/// The end of every [`step_tick`](crate::step_tick) (R72): empty every message buffer of
/// [`SimBuilder::add_message`](crate::SimBuilder::add_message). A message is read in the tick
/// it was written; none outlives its tick, so nothing is lost between ticks (saves).
pub(crate) fn clear_message_buffers(world: &mut World) {
    let per_tick = Arc::clone(&world.resource::<StateRegistry>().per_tick);
    for clear in &per_tick.message_clears {
        clear(world);
    }
}

/// Whether the type named `type_name` must be classified: one of the paths in its name,
/// generic arguments included, belongs to a sim crate (`fl_sim`, or an `fl_feat_*` crate that
/// is not a `*_client` crate), or it is a Bevy message buffer (`Messages<M>`), which only
/// [`SimBuilder::add_message`](crate::SimBuilder::add_message) adds to the sim: it classifies
/// the buffer and clears it every tick.
fn is_sim_type(type_name: &str) -> bool {
    type_name
        .split(|c: char| !(c.is_alphanumeric() || c == '_' || c == ':'))
        .any(|path| {
            let krate = path.split("::").next().unwrap_or(path);
            let sim_crate =
                (krate == "fl_sim" || krate.starts_with("fl_feat_")) && !krate.ends_with("_client");
            let message_buffer = krate == "bevy_ecs" && path.ends_with("::Messages");
            sim_crate || message_buffer
        })
}

/// The components no entity of the sim world may have: Bevy relationships hold `Entity`
/// (sim state refers to entities by `SimId`), and observers run outside the schedule (sim
/// crates use no observers; component hooks maintain indices only).
fn forbidden_components() -> [(TypeId, &'static str); 3] {
    [
        (TypeId::of::<ChildOf>(), std::any::type_name::<ChildOf>()),
        (TypeId::of::<Children>(), std::any::type_name::<Children>()),
        (TypeId::of::<Observer>(), std::any::type_name::<Observer>()),
    ]
}

/// Whether any entity of `world` has component `id`.
fn any_entity_has(world: &World, id: ComponentId) -> bool {
    world
        .archetypes()
        .iter()
        .any(|archetype| !archetype.is_empty() && archetype.contains(id))
}

/// One problem per forbidden component ([`forbidden_components`]) an entity of `world` has.
fn forbidden_in_world(world: &World) -> Vec<String> {
    forbidden_components()
        .into_iter()
        .filter(|&(type_id, _)| {
            world
                .components()
                .get_id(type_id)
                .is_some_and(|id| any_entity_has(world, id))
        })
        .map(|(_, name)| format!("{name} (not allowed in the sim world)"))
        .collect()
}

/// Check that every component and resource type of a sim crate registered in `world` is
/// classified ([`is_sim_type`]: a type of a sim crate, also as a generic argument, or a
/// message buffer), and that no entity has a component the sim world forbids (`ChildOf`,
/// `Children`, `Observer`).
///
/// # Errors
/// The names of the unclassified types, sorted, then the forbidden components found.
pub fn check_classification(world: &World) -> Result<(), Vec<String>> {
    let state = world.get_resource::<StateRegistry>();
    let mut unclassified: Vec<String> = world
        .components()
        .iter_registered()
        .filter_map(|(_, info)| {
            let name = info.name().to_string();
            if !is_sim_type(&name) {
                return None;
            }
            let classified = info
                .type_id()
                .zip(state)
                .is_some_and(|(type_id, state)| state.contains(type_id));
            (!classified).then_some(name)
        })
        .collect();
    unclassified.sort_unstable();
    unclassified.dedup();
    unclassified.extend(forbidden_in_world(world));
    if unclassified.is_empty() {
        Ok(())
    } else {
        Err(unclassified)
    }
}

/// How many component types (resources included) the sim world had when
/// [`check_classification`] last passed; [`check_tick`] checks again when there are more.
/// Transient (kept) `"core.classified_up_to"`: debug bookkeeping, never read by sim logic.
#[derive(Resource, Default, Debug)]
pub(crate) struct ClassifiedUpTo(pub(crate) usize);

/// Check that every entity with a classified component has a [`SimEntity`]: an entity
/// spawned around the spawn funnel would take part in gameplay yet never be hashed, saved
/// or loaded. Resources are entities in Bevy 0.20, so raw entity counts mean nothing here;
/// only entities holding a classified component count.
///
/// # Errors
/// One line per archetype at fault: how many entities, and their classified components.
pub(crate) fn check_sim_entities(world: &World) -> Result<(), Vec<String>> {
    let Some(state) = world.get_resource::<StateRegistry>() else {
        return Ok(());
    };
    let Some(sim_entity) = world.components().component_id::<SimEntity>() else {
        return Ok(());
    };
    // Runs every tick in debug builds: nothing here allocates unless a check fails.
    let problems: Vec<String> = world
        .archetypes()
        .iter()
        .filter(|archetype| !archetype.is_empty() && !archetype.contains(sim_entity))
        .filter_map(|archetype| {
            let names: Vec<&str> = state
                .components
                .iter()
                .filter(|(_, _, entry)| {
                    world
                        .components()
                        .get_id(entry.type_id)
                        .is_some_and(|id| id != sim_entity && archetype.contains(id))
                })
                .map(|(_, name, _)| name)
                .collect();
            (!names.is_empty()).then(|| {
                format!(
                    "{} entities with {} have no SimEntity",
                    archetype.len(),
                    names.join(", ")
                )
            })
        })
        .collect();
    if problems.is_empty() {
        Ok(())
    } else {
        Err(problems)
    }
}

/// The debug-build checks [`step_tick`](crate::step_tick) runs after each tick: when the
/// tick registered new component or resource types, [`check_classification`] again (a type
/// a spawn, a command or a deferred insert used first is registered only then); and always
/// [`check_sim_entities`] and the forbidden components (`ChildOf`, `Children`, `Observer`).
///
/// # Panics
/// Naming the unclassified types or forbidden components, or the entities without a
/// [`SimEntity`].
pub(crate) fn check_tick(world: &mut World) {
    let tick = world.resource::<crate::SimClock>().tick.0;
    let registered = world.components().len();
    if registered > world.resource::<ClassifiedUpTo>().0 {
        if let Err(names) = check_classification(world) {
            panic!(
                "tick {tick} registered unclassified sim types (register them with SimBuilder): \
                 {names:?}"
            );
        }
        world.resource_mut::<ClassifiedUpTo>().0 = registered;
    }
    // Every tick, not only when new types appear: a forbidden type registered at build (by a
    // system's query, say) and first instanced mid-match must fail too.
    let forbidden = forbidden_in_world(world);
    assert!(forbidden.is_empty(), "after tick {tick}: {forbidden:?}");
    if let Err(problems) = check_sim_entities(world) {
        panic!(
            "after tick {tick}: {problems:?}; spawn sim entities through SpawnRequest or \
             setup_spawn"
        );
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::Arc;

    use fl_protocol::StringTables;
    use serde::Deserialize;

    use super::*;
    use crate::{
        BuildError, MatchSetup, RunConfig, SimBuilder, SimFeature, build_sim_app, load_content,
    };

    #[derive(Resource, Default)]
    struct Stray;

    struct StrayFeature;

    impl SimFeature for StrayFeature {
        fn id(&self) -> &'static str {
            "stray"
        }

        fn version(&self) -> u32 {
            1
        }

        fn build(&self, sim: &mut SimBuilder) {
            sim.app_mut().init_resource::<Stray>();
        }
    }

    // The check runs in debug builds only.
    #[cfg(debug_assertions)]
    #[test]
    fn unclassified_sim_type_fails_build() {
        let features: Vec<Arc<dyn SimFeature>> = vec![Arc::new(StrayFeature)];
        let core_mod = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/core_mod");
        let content = load_content(&features, &[core_mod]).expect("core_mod loads");
        match build_sim_app(
            &features,
            content,
            MatchSetup::default(),
            RunConfig::default(),
        ) {
            Err(BuildError::Unclassified(names)) => {
                assert_eq!(names, ["fl_sim::classify::tests::Stray"]);
            }
            Err(other) => panic!("expected Unclassified, got {other}"),
            Ok(_) => panic!("expected Unclassified, the sim built"),
        }
    }

    #[test]
    fn sim_types_are_recognised_by_any_path_segment() {
        assert!(is_sim_type("fl_sim::units::Pose"));
        assert!(is_sim_type(
            "fl_sim::events::SimEvents<fl_view::event::SimEvent>"
        ));
        assert!(is_sim_type("fl_feat_artillery::Shell"));
        assert!(!is_sim_type("fl_feat_artillery_client::ShellMesh"));
        assert!(!is_sim_type("fl_simulator::Thing"));
        assert!(!is_sim_type("fl_content::registry::DefRegistry"));
        // Generic arguments count.
        assert!(is_sim_type("other::Wrapper<u8, fl_feat_artillery::Shell>"));
        assert!(is_sim_type("other::Pair<(u8, fl_sim::units::Pose)>"));
        assert!(is_sim_type("other::Ref<&'static fl_feat_x::A>"));
        assert!(!is_sim_type("other::Wrapper<fl_feat_x_client::Mesh>"));
        // Every message buffer: only SimBuilder::add_message classifies one.
        assert!(is_sim_type(
            "bevy_ecs::message::messages::Messages<fl_sim::spawn::SpawnRequest>"
        ));
        assert!(is_sim_type(
            "bevy_ecs::message::messages::Messages<other::Hit>"
        ));
        assert!(!is_sim_type("bevy_ecs::message::registry::MessageRegistry"));
    }

    #[derive(Resource)]
    struct Late;

    /// Inserts the unclassified resource `Late` through `Commands` in tick 1: a type first
    /// registered mid-match, after the build-time check.
    struct LateFeature;

    impl SimFeature for LateFeature {
        fn id(&self) -> &'static str {
            "late"
        }

        fn version(&self) -> u32 {
            1
        }

        fn build(&self, sim: &mut SimBuilder) {
            sim.add_systems(
                crate::ThinkSet::Orders,
                |clock: bevy_ecs::system::Res<crate::SimClock>,
                 mut commands: bevy_ecs::system::Commands| {
                    if clock.tick == fl_protocol::Tick(1) {
                        commands.insert_resource(Late);
                    }
                },
            );
        }
    }

    #[cfg(debug_assertions)]
    #[test]
    #[should_panic(expected = "tick 1 registered unclassified sim types")]
    fn late_registered_sim_type_fails_its_tick() {
        let features: Vec<Arc<dyn SimFeature>> = vec![Arc::new(LateFeature)];
        let core_mod = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/core_mod");
        let content = load_content(&features, &[core_mod]).expect("core_mod loads");
        let mut app = build_sim_app(
            &features,
            content,
            MatchSetup::default(),
            RunConfig::default(),
        )
        .expect("Late is not registered yet");
        for tick in 0..2 {
            let input = fl_view::TickInput {
                tick: fl_protocol::Tick(tick),
                commands: Vec::new(),
            };
            crate::step_tick(app.world_mut(), input);
        }
    }

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    struct Sample {
        a: u32,
        b: Vec<u8>,
    }

    impl SimHash for Sample {
        fn sim_hash(&self, h: &mut StateHasher) {
            self.a.sim_hash(h);
            self.b.sim_hash(h);
        }
    }

    #[test]
    fn hashed_codec_round_trips() {
        let Class::Hashed(codec) = Class::<Sample>::hashed(4) else {
            panic!("hashed() is Class::Hashed");
        };
        assert_eq!(codec.version, 4);
        let sample = Sample {
            a: 7,
            b: vec![1, 2],
        };
        let tables = StringTables::default();
        let bytes = (codec.encode)(&sample, &SaveCtx::new(&tables));
        let ctx = LoadCtx::new(&tables, &tables);
        assert_eq!((codec.decode)(4, &bytes, &ctx).expect("decodes"), sample);
        assert_eq!(
            codec.decode_saved(4, &bytes, &ctx).expect("decodes"),
            sample
        );
        assert!(matches!(
            codec.decode_saved(3, &bytes, &ctx),
            Err(CodecError::NoMigration { from: 3, to: 4 })
        ));
        let mut long = bytes.clone();
        long.push(0);
        assert!(matches!(
            codec.decode_saved(4, &long, &ctx),
            Err(CodecError::Codec(_))
        ));
        assert_eq!(
            (codec.dump)(&sample),
            serde_json::json!({ "a": 7, "b": [1, 2] })
        );
        let mut direct = StateHasher::new();
        sample.sim_hash(&mut direct);
        let mut via_codec = StateHasher::new();
        (codec.hash)(&sample, &mut via_codec);
        assert_eq!(direct.finish(), via_codec.finish());
    }

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    enum Kind {
        A,
        B(u8),
        C { x: u16 },
    }

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    struct Outer {
        kinds: Vec<Kind>,
    }

    impl SimHash for Outer {
        fn sim_hash(&self, _h: &mut StateHasher) {}
    }

    #[test]
    fn codec_traces_nested_enums() {
        let codec = Codec::<Outer>::new(1);
        let err = codec.formats().expect_err("Kind is nested in a Vec");
        assert!(err.to_string().contains("Kind"), "{err}");

        let (root, registry) = codec
            .trace_enum::<Kind>()
            .formats()
            .expect("Outer traces with Kind");
        assert_eq!(root, Format::TypeName("Outer".into()));
        let Some(serde_reflection::ContainerFormat::Enum(variants)) = registry.get("Kind") else {
            panic!("Kind is an enum: {registry:?}");
        };
        let names: Vec<(u32, &str)> = variants
            .iter()
            .map(|(&index, variant)| (index, variant.name.as_str()))
            .collect();
        assert_eq!(names, [(0, "A"), (1, "B"), (2, "C")]);
    }
}
