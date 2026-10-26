//! The [`SimTick`] schedule: eight [`Phase`]s, each split into public hook sub-sets
//! (spec §5.6, ADR-0005).
//!
//! The phases run `Input → Think → Act → Resolve → Economy → Perceive → Lifecycle → Publish`.
//! Inside a phase its hooks run in declared order, and every phase ends with one explicit
//! [`ApplyDeferred`], so `Commands` queued during a phase become visible at the next phase,
//! whatever other features are linked. Features add systems only into the hooks, through
//! [`SimBuilder::add_systems`](crate::SimBuilder::add_systems). Each hook has a private core
//! slot that runs first (core systems, some of them exclusive) and a feature slot after it.
//!
//! The executor is single-threaded, ambiguity detection is at error level (two systems with
//! conflicting access and no order between them make the schedule fail to build), and
//! `auto_insert_apply_deferred` is off. Run the schedule only through
//! [`step_tick`](crate::step_tick).

use std::collections::BTreeMap;
use std::fmt::Write as _;

use bevy_app::App;
use bevy_ecs::schedule::graph::Direction;
use bevy_ecs::schedule::{
    ApplyDeferred, InternedSystemSet, IntoScheduleConfigs, LogLevel, NodeId, Schedule,
    ScheduleBuildSettings, ScheduleLabel, SingleThreadedExecutor, SystemSet,
};
use bevy_ecs::world::World;

/// One simulation tick. Only [`step_tick`](crate::step_tick) runs it.
#[derive(ScheduleLabel, Debug, Clone, PartialEq, Eq, Hash)]
pub struct SimTick;

/// The phases of a tick, in run order.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Phase {
    /// Housekeeping (the `Transient` resets), then this tick's commands.
    Input,
    /// Orders into activities, target acquisition, unit-level reflexes.
    Think,
    /// Movement and firing intents.
    Act,
    /// Projectiles, effects, conditions, modifier recompute.
    Resolve,
    /// Resources, production, construction, research.
    Economy,
    /// Spatial-index maintenance, vision.
    Perceive,
    /// Death, the spawn funnel, despawn.
    Lifecycle,
    /// Snapshot and events.
    Publish,
}

const PHASES: [Phase; 8] = [
    Phase::Input,
    Phase::Think,
    Phase::Act,
    Phase::Resolve,
    Phase::Economy,
    Phase::Perceive,
    Phase::Lifecycle,
    Phase::Publish,
];

mod sealed {
    pub trait Sealed {}
}

/// A hook sub-set of one [`Phase`]: the only places features add systems. Implemented by
/// exactly the eight `*Set` enums of this module; a new hook is a contract change.
pub trait SimHook: SystemSet + Copy + sealed::Sealed {
    /// The phase this hook runs in.
    fn phase(self) -> Phase;
}

macro_rules! hooks {
    ($($(#[$meta:meta])* $name:ident in $phase:ident { $($(#[$vmeta:meta])* $variant:ident,)+ })+) => {
        $(
            $(#[$meta])*
            #[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
            pub enum $name {
                $($(#[$vmeta])* $variant,)+
            }

            impl $name {
                /// The hooks in run order.
                const ALL: &'static [$name] = &[$($name::$variant,)+];
            }

            impl sealed::Sealed for $name {}

            impl SimHook for $name {
                fn phase(self) -> Phase {
                    Phase::$phase
                }
            }
        )+
    };
}

hooks! {
    /// The hooks of [`Phase::Input`].
    InputSet in Input {
        /// The `Transient` resets: every `Transient` component removed, every resettable
        /// `Transient` resource reset (R72).
        Housekeeping,
        /// Command decoding and validation.
        Validate,
        /// Applying this tick's commands.
        Apply,
    }
    /// The hooks of [`Phase::Think`].
    ThinkSet in Think {
        /// Orders into activities.
        Orders,
        /// Per-unit decisions: targets, wandering, stances.
        Behaviour,
    }
    /// The hooks of [`Phase::Act`].
    ActSet in Act {
        Move,
        Fire,
    }
    /// The hooks of [`Phase::Resolve`].
    ResolveSet in Resolve {
        Projectiles,
        Effects,
        Stats,
    }
    /// The hooks of [`Phase::Economy`].
    EconomySet in Economy {
        Main,
    }
    /// The hooks of [`Phase::Perceive`].
    PerceiveSet in Perceive {
        Spatial,
        Vision,
    }
    /// The hooks of [`Phase::Lifecycle`].
    LifecycleSet in Lifecycle {
        Death,
        Spawn,
        Despawn,
    }
    /// The hooks of [`Phase::Publish`].
    PublishSet in Publish {
        Snapshot,
        Events,
    }
}

/// The end of a phase: holds its one [`ApplyDeferred`]. Not a hook; nothing else goes here.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct PhaseSync(Phase);

/// One of the two slots of a hook (ADR-0005, R46): the core systems of the hook run first,
/// in the core slot, then the features' systems, in the feature slot. So a feature system
/// never conflicts with an exclusive core system of its hook (command apply, the `Transient`
/// resets, the spawn funnel), and sees its effects.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct HookSlot {
    hook: InternedSystemSet,
    core: bool,
}

/// The set that systems added to `hook` go in: its core slot for core systems
/// (`core == true`), else its feature slot.
pub(crate) fn hook_slot(hook: impl SimHook, core: bool) -> impl SystemSet {
    HookSlot {
        hook: hook.intern(),
        core,
    }
}

/// Put `hooks` in their phase, in order, each with its core slot before its feature slot,
/// followed by the phase's sync point.
fn configure_phase<H: SimHook>(schedule: &mut Schedule, hooks: &[H]) {
    let phase = hooks[0].phase();
    for &hook in hooks {
        schedule.configure_sets(hook.in_set(phase));
        schedule.configure_sets(hook_slot(hook, true).in_set(hook));
        schedule.configure_sets(
            hook_slot(hook, false)
                .in_set(hook)
                .after(hook_slot(hook, true)),
        );
    }
    for pair in hooks.windows(2) {
        schedule.configure_sets(pair[1].after(pair[0]));
    }
    let last = hooks[hooks.len() - 1];
    schedule.configure_sets(PhaseSync(phase).in_set(phase).after(last));
    schedule.add_systems(ApplyDeferred.in_set(PhaseSync(phase)));
}

/// Create [`SimTick`]: a single-threaded executor, error-level ambiguity detection, no
/// automatic sync points, `shuffle_seed` (shuffles the order of unordered systems, for
/// determinism checks), the chained phases, the chained hooks inside each phase, and one
/// [`ApplyDeferred`] ending each phase.
pub fn configure_sim_schedule(app: &mut App, shuffle_seed: Option<u64>) {
    app.edit_schedule(SimTick, |schedule| {
        schedule
            .set_executor(SingleThreadedExecutor::new())
            .set_build_settings(ScheduleBuildSettings {
                ambiguity_detection: LogLevel::Error,
                auto_insert_apply_deferred: false,
                shuffle_seed,
                ..Default::default()
            })
            .configure_sets(
                (
                    Phase::Input,
                    Phase::Think,
                    Phase::Act,
                    Phase::Resolve,
                    Phase::Economy,
                    Phase::Perceive,
                    Phase::Lifecycle,
                    Phase::Publish,
                )
                    .chain(),
            );
        configure_phase(schedule, InputSet::ALL);
        configure_phase(schedule, ThinkSet::ALL);
        configure_phase(schedule, ActSet::ALL);
        configure_phase(schedule, ResolveSet::ALL);
        configure_phase(schedule, EconomySet::ALL);
        configure_phase(schedule, PerceiveSet::ALL);
        configure_phase(schedule, LifecycleSet::ALL);
        configure_phase(schedule, PublishSet::ALL);
    });
}

/// Build [`SimTick`] if it is not built yet.
///
/// # Panics
/// If the schedule does not build (an ambiguity, a cycle), with Bevy's report.
pub(crate) fn initialize_sim_tick(world: &mut World) {
    world.schedule_scope(SimTick, |world, schedule| {
        if let Err(err) = schedule.initialize(world) {
            panic!(
                "SimTick schedule failed to build: {}",
                err.to_string(schedule.graph(), world)
            );
        }
    });
}

fn push_labels<H: SimHook>(labels: &mut Vec<(InternedSystemSet, String)>, hooks: &[H]) {
    for &hook in hooks {
        labels.push((hook.intern(), format!("{:?}/{hook:?}", hook.phase())));
    }
}

/// The hook (`"<Phase>/<Hook>"`) or sync point (`"<Phase>/Sync"`) of every set that is one.
fn set_labels() -> Vec<(InternedSystemSet, String)> {
    let mut labels = Vec::new();
    push_labels(&mut labels, InputSet::ALL);
    push_labels(&mut labels, ThinkSet::ALL);
    push_labels(&mut labels, ActSet::ALL);
    push_labels(&mut labels, ResolveSet::ALL);
    push_labels(&mut labels, EconomySet::ALL);
    push_labels(&mut labels, PerceiveSet::ALL);
    push_labels(&mut labels, LifecycleSet::ALL);
    push_labels(&mut labels, PublishSet::ALL);
    for phase in PHASES {
        labels.push((PhaseSync(phase).intern(), format!("{phase:?}/Sync")));
    }
    labels
}

/// The built [`SimTick`], one line per system in executed order: `"<Phase>/<Hook> <system
/// name>"` (`"<Phase>/Sync"` for a phase's [`ApplyDeferred`], `"?"` for a system outside every
/// hook). Inside a hook, the core systems come first. Builds the schedule first.
///
/// # Panics
/// If the schedule does not build.
pub fn system_order(app: &mut App) -> String {
    let world = app.world_mut();
    initialize_sim_tick(world);
    world.schedule_scope(SimTick, |_, schedule| {
        let graph = schedule.graph();
        let labels: BTreeMap<_, String> = set_labels()
            .into_iter()
            .filter_map(|(set, label)| Some((graph.system_sets.get_key(set)?, label)))
            .collect();
        let hierarchy = graph.hierarchy().graph();
        // The nearest enclosing hook or sync set, searching parents breadth-first.
        let label_of = |node: NodeId| -> &str {
            let mut frontier = vec![node];
            while !frontier.is_empty() {
                let mut parents = Vec::new();
                for node in frontier {
                    for parent in hierarchy.neighbors_directed(node, Direction::Incoming) {
                        if let NodeId::Set(key) = parent
                            && let Some(label) = labels.get(&key)
                        {
                            return label;
                        }
                        parents.push(parent);
                    }
                }
                frontier = parents;
            }
            "?"
        };

        let mut out = String::new();
        for (key, system) in schedule.systems().expect("SimTick is initialized") {
            let label = label_of(NodeId::System(key));
            writeln!(out, "{label} {}", system.name()).expect("writing to a String");
        }
        out
    })
}
