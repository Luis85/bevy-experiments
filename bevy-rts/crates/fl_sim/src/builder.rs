//! [`SimBuilder`]: everything a [`SimFeature`](crate::SimFeature) registers on the sim
//! (spec §5.2, ADR-0001, ADR-0004, ADR-0006).
//!
//! Each registration is recorded under the id of the feature being built, so a duplicate name
//! panics naming both features. When the sim is built every registry is frozen: names are
//! sorted, dense ids assigned, and the names exported as the `MatchHeader`'s string tables.

use std::any::TypeId;
use std::collections::BTreeSet;

use bevy_app::App;
use bevy_ecs::component::Component;
use bevy_ecs::message::{Message, Messages};
use bevy_ecs::resource::Resource;
use bevy_ecs::schedule::IntoScheduleConfigs;
use bevy_ecs::system::ScheduleSystem;
use bevy_ecs::world::World;
use fl_content::{ContentKind, DefComponent, DefRegistry, RulesSection};
use fl_protocol::{CommandPayload, StringTables, tables};
use serde::de::DeserializeOwned;

use crate::classify::{Class, StateRegistry};
use crate::commands::{CommandEntry, CommandHandler, CommandRegistry};
use crate::counters::SimCounters;
use crate::debug_query::{DebugQueries, DebugQueryFn};
use crate::feature::{CoreFeature, SimFeature};
use crate::observe::{Journal, JournalRecord};
use crate::registry::StableRegistry;
use crate::rng::Stream;
use crate::schedule::{SimHook, SimTick, hook_slot};

/// Registers a [`SimFeature`](crate::SimFeature)'s content names, state classes, commands,
/// RNG streams, journal record kinds, debug queries and systems. Only
/// [`build_sim_app`](crate::build_sim_app) and [`sim_registry`](crate::sim_registry) create
/// one.
pub struct SimBuilder {
    pub(crate) app: App,
    feature: &'static str,
    defs: DefRegistry,
    commands: StableRegistry<CommandEntry>,
    state: StateRegistry,
    streams: StableRegistry<()>,
    journal_records: StableRegistry<()>,
    debug_queries: StableRegistry<DebugQueryFn>,
    /// The message types added so far.
    messages: BTreeSet<TypeId>,
}

impl SimBuilder {
    pub(crate) fn new() -> Self {
        SimBuilder {
            app: App::empty(),
            feature: "",
            defs: DefRegistry::with_builtins(),
            commands: StableRegistry::new(tables::COMMANDS),
            state: StateRegistry::default(),
            streams: StableRegistry::new(tables::RNG_STREAMS),
            journal_records: StableRegistry::new(tables::JOURNAL_RECORDS),
            debug_queries: crate::debug_query::registry(),
            messages: BTreeSet::new(),
        }
    }

    /// Run `feature`'s [`build`](SimFeature::build), recording its registrations under its id.
    pub(crate) fn build_feature(&mut self, feature: &dyn SimFeature) {
        self.feature = feature.id();
        feature.build(self);
        self.feature = "";
    }

    /// The content registry the features filled, not yet frozen.
    pub(crate) fn into_def_registry(self) -> DefRegistry {
        self.defs
    }

    /// Freeze every registry, insert the registries the running sim needs as resources, and
    /// finish the app. Returns the app and every registry's string table.
    ///
    /// Allows scheduling ambiguities on [`SimCounters`]: counter adds commute.
    ///
    /// Also inserts the (disabled) [`Journal`] over the journal record kinds, classified
    /// Transient (kept: [`step_tick`](crate::step_tick) drains it) `"core.journal"`, with its
    /// scheduling ambiguities allowed: systems record in any order, and `step_tick` sorts each
    /// tick's records; and the [`DebugQueries`], classified External `"core.debug_queries"`
    /// and, unlike the other registries, not exported to the string tables.
    ///
    /// # Panics
    /// If more command types are registered than a `CommandTypeId` can number.
    pub(crate) fn finish(mut self) -> (App, StringTables) {
        self.state.register_resource::<Journal>(
            "core.journal",
            CoreFeature::ID,
            Class::TransientKept("drained by step_tick into each tick's output"),
        );
        self.state.register_resource::<DebugQueries>(
            "core.debug_queries",
            CoreFeature::ID,
            Class::External,
        );
        self.defs.freeze();
        self.commands.freeze();
        self.state.freeze();
        self.streams.freeze();
        self.journal_records.freeze();
        self.debug_queries.freeze();

        let mut tables = self.defs.string_tables();
        self.commands.export(&mut tables);
        self.state.components.export(&mut tables);
        self.state.resources.export(&mut tables);
        self.streams.export(&mut tables);
        self.journal_records.export(&mut tables);

        self.app
            .insert_resource(CommandRegistry::new(self.commands))
            .insert_resource(Journal::new(self.journal_records))
            .insert_resource(DebugQueries::new(self.debug_queries))
            .allow_ambiguous_resource::<Journal>()
            // Spec §15: every mechanic counts its work, and counter adds commute (a sum per
            // name), so counting systems need no order among themselves. Sim logic never
            // reads the counters.
            .allow_ambiguous_resource::<SimCounters>()
            .insert_resource(self.state)
            .insert_resource(self.defs);
        self.app.finish();
        self.app.cleanup();
        (self.app, tables)
    }

    /// The id of the feature being built.
    pub fn feature_id(&self) -> &'static str {
        self.feature
    }

    /// A def component that unit files can use under `name` (spec §6.1).
    ///
    /// # Panics
    /// If `name` or the type `T` is already registered.
    pub fn register_def_component<T: DefComponent + DeserializeOwned>(
        &mut self,
        name: &'static str,
    ) -> &mut Self {
        self.defs.register_component::<T>(name);
        self
    }

    /// A content kind, loaded from `*.<K::EXT>.ron` files.
    ///
    /// # Panics
    /// If `K::EXT` is already registered.
    pub fn register_content_kind<K: ContentKind>(&mut self) -> &mut Self {
        self.defs.register_kind::<K>();
        self
    }

    /// A required `rules.ron` section.
    ///
    /// # Panics
    /// If `name` or the type `S` is already registered.
    pub fn register_rules_section<S: RulesSection>(&mut self, name: &'static str) -> &mut Self {
        self.defs.register_rules_section::<S>(name);
        self
    }

    /// Classify component `C` as `name` (`"<feature>.<component>"`). A [`Class::Hashed`]
    /// component feeds the state hash and [`inspect_entity`](crate::inspect_entity); a
    /// [`Class::Transient`] one is removed from every entity at the start of each tick.
    ///
    /// # Panics
    /// If `class` is [`Class::External`] ("External is for resources"),
    /// [`Class::TransientReset`] or [`Class::TransientKept`] (resources only), `C` is already
    /// classified, or `name` is taken (naming both features).
    pub fn register_component<C: Component>(
        &mut self,
        name: &'static str,
        class: Class<C>,
    ) -> &mut Self {
        self.state.register_component(name, self.feature, class);
        self
    }

    /// The feature's one `Derived` rebuild: after a load has restored every `Hashed` value,
    /// [`load_world`](crate::load_world) runs each feature's rebuild in feature load order
    /// (core first, then every feature after its dependencies). It rebuilds all of the
    /// feature's [`Class::Derived`] state, in the order that state depends on itself (a
    /// cache after the grid it caches).
    ///
    /// # Panics
    /// If the feature already registered one.
    pub fn register_rebuild(&mut self, rebuild: fn(&mut World)) -> &mut Self {
        self.state.register_rebuild(self.feature, rebuild);
        self
    }

    /// Classify resource `R` as `name`. Inserting the resource is up to the feature. A
    /// `Transient` resource is [`Class::transient`] or [`Class::TransientReset`] (reset at the
    /// start of each tick) or [`Class::TransientKept`] (left alone, for the named reason).
    ///
    /// # Panics
    /// If `class` is plain [`Class::Transient`] (components only), `R` is already classified,
    /// or `name` is taken (naming both features).
    pub fn register_resource<R: Resource>(
        &mut self,
        name: &'static str,
        class: Class<R>,
    ) -> &mut Self {
        self.state.register_resource(name, self.feature, class);
        self
    }

    /// Command payload `P` (named [`CommandPayload::NAME`]), carried out by `handler`. Command
    /// type ids follow name order.
    ///
    /// # Panics
    /// If `P::NAME` is already registered (naming both features).
    pub fn register_command<P: CommandPayload>(
        &mut self,
        handler: impl CommandHandler<P>,
    ) -> &mut Self {
        self.commands
            .register(P::NAME, self.feature, CommandEntry::new::<P>(handler));
        self
    }

    /// A random stream `name`: the [`Stream`] that [`rolls`](crate::rolls) draws from, keyed
    /// `fl_math::stream_key(name)`. Hand it to the systems that draw (a closure capture, or
    /// `IntoSystem::with_input`).
    ///
    /// # Panics
    /// If `name` is already registered (naming both features).
    #[must_use = "rolls draws only from the returned Stream"]
    pub fn register_stream(&mut self, name: &'static str) -> Stream {
        self.streams.register(name, self.feature, ());
        Stream::named(name)
    }

    /// Journal record kind `R` (named [`JournalRecord::KIND`], `"<feature>.<record>"`), so
    /// systems may [`record`](Journal::record) it. Kinds are exported as the
    /// `"journal_records"` string table.
    ///
    /// # Panics
    /// If `R::KIND` is already registered (naming both features).
    pub fn register_journal_record<R: JournalRecord>(&mut self) -> &mut Self {
        self.journal_records.register(R::KIND, self.feature, ());
        self
    }

    /// Debug query `name` (`"<feature>.<query>"`, ADR-0015): `query` answers
    /// [`SimQuery::Debug`](fl_view::SimQuery::Debug) between ticks, read-only, in JSON with
    /// `Fx` as its raw `i64`. Sim logic never runs it, and its answers are never hashed. Core
    /// registers `"core.debug_queries"`, which lists every name.
    ///
    /// `query` must never panic: it answers `Err` for bad or missing args and for a world
    /// without the state it reads. The threaded runner runs it on the sim thread, where a
    /// panic ends the match.
    ///
    /// # Panics
    /// If `name` is already registered (naming both features).
    pub fn register_debug_query(&mut self, name: &'static str, query: DebugQueryFn) -> &mut Self {
        self.debug_queries.register(name, self.feature, query);
        self
    }

    /// Add `systems` to [`SimTick`] in `hook`, one of the phase hook sub-sets
    /// ([`InputSet`], [`ThinkSet`](crate::ThinkSet), ...). A feature's systems run after the
    /// hook's core systems; [`CoreFeature`](crate::CoreFeature)'s go in the core slot, first.
    /// Systems in one slot run in any order unless ordered among themselves; the schedule
    /// fails to build if two of them conflict without an order.
    pub fn add_systems<M>(
        &mut self,
        hook: impl SimHook,
        systems: impl IntoScheduleConfigs<ScheduleSystem, M>,
    ) -> &mut Self {
        let core = self.feature == CoreFeature::ID;
        self.add_systems_in_slot(hook, core, systems)
    }

    fn add_systems_in_slot<M>(
        &mut self,
        hook: impl SimHook,
        core: bool,
        systems: impl IntoScheduleConfigs<ScheduleSystem, M>,
    ) -> &mut Self {
        self.app
            .add_systems(SimTick, systems.in_set(hook_slot(hook, core)));
        self
    }

    /// Message type `M`, under `name` (`"<feature>.msg.<message>"`): inserts its [`Messages`]
    /// buffer, classified Transient as `name`, which [`step_tick`](crate::step_tick) clears at
    /// the end of every tick (R72). A message is read in the tick it was written: one written
    /// after its readers ran is dropped, so state that must reach the next tick is `Hashed`.
    /// Adding `M` again (under any name) does nothing. The only way to add a message buffer to
    /// the sim: one added around the builder is an unclassified sim type.
    ///
    /// # Panics
    /// If `name` is already registered as a resource.
    pub fn add_message<M: Message>(&mut self, name: &'static str) -> &mut Self {
        if !self.messages.insert(TypeId::of::<M>()) {
            return self;
        }
        self.app.init_resource::<Messages<M>>();
        self.state.clear_messages_each_tick::<M>();
        self.register_resource::<Messages<M>>(
            name,
            Class::TransientKept("cleared at the end of every step_tick"),
        )
    }

    /// The sim app itself: the escape hatch for what the builder does not cover (inserting
    /// resources, configuring sets). Review flags every use.
    pub fn app_mut(&mut self) -> &mut App {
        &mut self.app
    }
}
