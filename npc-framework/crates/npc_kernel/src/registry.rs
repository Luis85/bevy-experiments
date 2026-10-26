//! Stable-name registration and the freeze.
//!
//! Content registers by **name**: actions, conditions, scorers and trees
//! (`npc_ai::AiRegistry`), needs (`npc_needs`), sim components and resources
//! ([`SimHashRegistry`]), content commands ([`CommandRegistry`]), kind names
//! and leave hooks. [`NpcKernelPlugin`](crate::NpcKernelPlugin)'s
//! `Plugin::finish` calls [`ensure_frozen`] once: every registry assigns its
//! ids sorted by name and resolves references, then the [`ContentHash`] over
//! all registered names (plus every tree's content hash) is computed. So ids,
//! state hashes and replays never depend on plugin or registration order.
//! Registering after the freeze panics.

use bevy_app::App;
use bevy_ecs::prelude::*;
use bevy_reflect::{GetTypeRegistration, Reflect};
use npc_ai::{AiRegistry, CondFn, ScoreFn, StartFn, StopFn, TreeBuilder};
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::commands::{CommandRegistry, ContentCommand};
use crate::hash::{Fnv64, JsonComponentFn, SimHash, SimHashRegistry, SimHashResource};
use crate::ids::{OnLeaveFn, SimIdHooks};
use crate::inspect::{Inspector, KindNameFn, KindNames};
use crate::snapshot::SnapshotRegistry;

/// A crate's freeze step: freezes its registry and returns the names it
/// contributes to the content hash as `(category, name)`.
pub type FreezeFn = fn(&mut World) -> Vec<(String, String)>;

/// Freeze steps of the generic crates, by name (run in name order).
#[derive(Resource, Clone, Default)]
pub struct FreezeHooks {
    pub hooks: Vec<(&'static str, FreezeFn)>,
}

impl FreezeHooks {
    pub fn add(&mut self, name: &'static str, f: FreezeFn) {
        match self.hooks.binary_search_by_key(&name, |(n, _)| n) {
            Ok(i) => self.hooks[i].1 = f,
            Err(i) => self.hooks.insert(i, (name, f)),
        }
    }
}

/// Rewrites a registered tree in place just before the freeze resolves its
/// names (e.g. `npc_agent_io` wraps every tree's root in a reserved
/// top-priority guard). Must be deterministic and only add registered names.
pub type TreeTransformFn = fn(&mut npc_ai::TreeDef);

/// Tree transforms of the generic crates, by name (applied in name order to
/// every tree, before the freeze; the content hash covers the result). Empty
/// unless a plugin opts in, so trees are unchanged by default.
#[derive(Resource, Clone, Default)]
pub struct TreeTransforms {
    pub transforms: Vec<(&'static str, TreeTransformFn)>,
}

impl TreeTransforms {
    /// Adds (or replaces) transform `name`.
    pub fn add(&mut self, name: &'static str, f: TreeTransformFn) {
        match self.transforms.binary_search_by_key(&name, |(n, _)| n) {
            Ok(i) => self.transforms[i].1 = f,
            Err(i) => self.transforms.insert(i, (name, f)),
        }
    }
}

/// Registers tree transform `name` (see [`TreeTransforms`]).
///
/// # Panics
/// When content is already frozen.
pub fn add_tree_transform(world: &mut World, name: &'static str, f: TreeTransformFn) {
    assert_open(world, &format!("tree transform `{name}`"));
    world.get_resource_or_init::<TreeTransforms>().add(name, f);
}

/// Set once frozen: the hash over every registered name and tree.
#[derive(Resource, Clone, Debug, PartialEq, Eq)]
pub struct ContentHash {
    pub hash: u64,
    /// Every `(category, name)` the hash covers, sorted.
    pub manifest: Vec<(String, String)>,
}

/// True once [`ensure_frozen`] ran.
pub fn is_frozen(world: &World) -> bool {
    world.contains_resource::<ContentHash>()
}

/// Freezes every registry (idempotent): AI ids sorted by name and trees
/// resolved, the generic crates' freeze hooks, then the [`ContentHash`].
///
/// # Panics
/// When a tree references an unknown name (the message lists the known ones).
pub fn ensure_frozen(world: &mut World) {
    if is_frozen(world) {
        return;
    }
    let mut manifest: Vec<(String, String)> = Vec::new();
    let transforms = world
        .get_resource::<TreeTransforms>()
        .map(|t| t.transforms.clone())
        .unwrap_or_default();
    {
        let mut ai = world.get_resource_or_init::<AiRegistry>();
        if !ai.is_frozen() {
            for (_, transform) in &transforms {
                for tree in &mut ai.trees {
                    transform(tree);
                }
            }
        }
        ai.freeze();
        manifest.extend(ai.manifest().into_iter().map(|(k, n)| (k.to_owned(), n)));
        for tree in &ai.trees {
            manifest.push((
                "tree_hash".to_owned(),
                format!("{}={:016x}", tree.name, tree.content_hash()),
            ));
        }
    }
    let hooks = world.get_resource_or_init::<FreezeHooks>().hooks.clone();
    for (_, hook) in hooks {
        manifest.extend(hook(world));
    }
    if let Some(r) = world.get_resource::<SimHashRegistry>() {
        manifest.extend(r.manifest().into_iter().map(|(k, n)| (k.to_owned(), n)));
    }
    if let Some(r) = world.get_resource::<CommandRegistry>() {
        manifest.extend(
            r.commands
                .keys()
                .map(|k| ("command".to_owned(), (*k).to_owned())),
        );
    }
    manifest.sort();
    manifest.dedup();
    let mut h = Fnv64::default();
    h.u32(u32::try_from(manifest.len()).expect("manifest too large"));
    for (category, name) in &manifest {
        h.str(category);
        h.str(name);
    }
    world.insert_resource(ContentHash {
        hash: h.0,
        manifest,
    });
}

/// The content hash (freezing first if needed).
pub fn content_hash(world: &mut World) -> u64 {
    ensure_frozen(world);
    world.resource::<ContentHash>().hash
}

fn assert_open(world: &World, what: &str) {
    assert!(
        !is_frozen(world),
        "cannot register {what}: content is already frozen (register in Plugin::build)"
    );
}

/// Kernel registration API on `App` (the facade's `NpcAppExt` adds needs).
/// Every method may be called before or after the kernel plugin is added,
/// in any order; ids are assigned by name at freeze time.
pub trait KernelAppExt {
    /// Registers a behaviour-tree action by stable name.
    fn register_action(&mut self, name: &'static str, start: StartFn, stop: StopFn) -> &mut Self;
    /// Registers a condition by stable name.
    fn register_cond(&mut self, name: &'static str, f: CondFn) -> &mut Self;
    /// Registers a utility scorer by stable name.
    fn register_scorer(&mut self, name: &'static str, f: ScoreFn) -> &mut Self;
    /// Builds and registers tree `name`; leaves name their actions/conditions.
    fn register_tree(&mut self, name: &'static str, build: fn(&mut TreeBuilder)) -> &mut Self;
    /// Adds `T` to the state hash, inspection, diffs, the reflection registry
    /// and world snapshots (under `T::NAME`; ADR 0013).
    fn register_sim_component<T>(&mut self) -> &mut Self
    where
        T: Component + SimHash + Serialize + DeserializeOwned + Reflect + GetTypeRegistration;
    /// [`KernelAppExt::register_sim_component`] with a custom JSON function
    /// (snapshots still use `T`'s serde form).
    fn register_sim_component_with<T>(&mut self, json: JsonComponentFn) -> &mut Self
    where
        T: Component + SimHash + Serialize + DeserializeOwned + Reflect + GetTypeRegistration;
    /// Adds resource `R` to the state hash, and to world snapshots (under
    /// `R::NAME`) when `R` opts in through [`SimHashResource::snapshot_fns`].
    fn register_sim_resource<R: SimHashResource>(&mut self) -> &mut Self;
    /// Registers a content command, dispatched by `C::NAME`.
    fn register_command<C: ContentCommand>(&mut self) -> &mut Self;
    /// Adds a unit-kind namer (journal `Spawn`, inspection `kind`).
    fn register_kind_name(&mut self, name: &'static str, f: KindNameFn) -> &mut Self;
    /// Runs `hook` whenever a `SimId` leaves the sim.
    fn on_sim_id_leave(&mut self, name: &'static str, hook: OnLeaveFn) -> &mut Self;
    /// Adds a generic crate's freeze step.
    fn add_freeze_hook(&mut self, name: &'static str, f: FreezeFn) -> &mut Self;
    /// Replaces the unit/world inspector (inspect JSON, BRP `npc/unit`/`npc/state`).
    fn set_inspector(&mut self, inspector: Inspector) -> &mut Self;
}

impl KernelAppExt for App {
    fn register_action(&mut self, name: &'static str, start: StartFn, stop: StopFn) -> &mut Self {
        let world = self.world_mut();
        assert_open(world, &format!("action `{name}`"));
        world
            .get_resource_or_init::<AiRegistry>()
            .add_action(name, start, stop);
        self
    }

    fn register_cond(&mut self, name: &'static str, f: CondFn) -> &mut Self {
        let world = self.world_mut();
        assert_open(world, &format!("condition `{name}`"));
        world.get_resource_or_init::<AiRegistry>().add_cond(name, f);
        self
    }

    fn register_scorer(&mut self, name: &'static str, f: ScoreFn) -> &mut Self {
        let world = self.world_mut();
        assert_open(world, &format!("scorer `{name}`"));
        world
            .get_resource_or_init::<AiRegistry>()
            .add_scorer(name, f);
        self
    }

    fn register_tree(&mut self, name: &'static str, build: fn(&mut TreeBuilder)) -> &mut Self {
        let world = self.world_mut();
        assert_open(world, &format!("tree `{name}`"));
        let mut b = TreeBuilder::new(name);
        build(&mut b);
        world
            .get_resource_or_init::<AiRegistry>()
            .add_tree(b.build());
        self
    }

    fn register_sim_component<T>(&mut self) -> &mut Self
    where
        T: Component + SimHash + Serialize + DeserializeOwned + Reflect + GetTypeRegistration,
    {
        assert_open(self.world(), &format!("sim component `{}`", T::NAME));
        self.register_type::<T>();
        self.world_mut()
            .get_resource_or_init::<SimHashRegistry>()
            .add_component::<T>();
        self.world_mut()
            .get_resource_or_init::<SnapshotRegistry>()
            .add_component::<T>(T::NAME);
        self
    }

    fn register_sim_component_with<T>(&mut self, json: JsonComponentFn) -> &mut Self
    where
        T: Component + SimHash + Serialize + DeserializeOwned + Reflect + GetTypeRegistration,
    {
        assert_open(self.world(), &format!("sim component `{}`", T::NAME));
        self.register_type::<T>();
        self.world_mut()
            .get_resource_or_init::<SimHashRegistry>()
            .add_component_with::<T>(json);
        self.world_mut()
            .get_resource_or_init::<SnapshotRegistry>()
            .add_component::<T>(T::NAME);
        self
    }

    fn register_sim_resource<R: SimHashResource>(&mut self) -> &mut Self {
        assert_open(self.world(), &format!("sim resource `{}`", R::NAME));
        self.world_mut()
            .get_resource_or_init::<SimHashRegistry>()
            .add_resource::<R>();
        if let Some(fns) = R::snapshot_fns() {
            self.world_mut()
                .get_resource_or_init::<SnapshotRegistry>()
                .add_resource_fns(R::NAME, fns.save, fns.load);
        }
        self
    }

    fn register_command<C: ContentCommand>(&mut self) -> &mut Self {
        assert_open(self.world(), &format!("command `{}`", C::NAME));
        self.world_mut()
            .get_resource_or_init::<CommandRegistry>()
            .add::<C>();
        self
    }

    fn register_kind_name(&mut self, name: &'static str, f: KindNameFn) -> &mut Self {
        self.world_mut()
            .get_resource_or_init::<KindNames>()
            .add(name, f);
        self
    }

    fn on_sim_id_leave(&mut self, name: &'static str, hook: OnLeaveFn) -> &mut Self {
        self.world_mut()
            .get_resource_or_init::<SimIdHooks>()
            .add(name, hook);
        self
    }

    fn add_freeze_hook(&mut self, name: &'static str, f: FreezeFn) -> &mut Self {
        self.world_mut()
            .get_resource_or_init::<FreezeHooks>()
            .add(name, f);
        self
    }

    fn set_inspector(&mut self, inspector: Inspector) -> &mut Self {
        self.world_mut().insert_resource(inspector);
        self
    }
}
