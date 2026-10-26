//! The `SimTick` of the game's feature set: the snapshot is the last word on the state it
//! publishes.

use std::sync::Arc;

use bevy_ecs::component::{Component, ComponentId};
use bevy_ecs::schedule::IntoScheduleConfigs;
use bevy_ecs::system::{Commands, Query, System};
use bevy_ecs::world::World;
use fl_sim::{
    Health, Invulnerable, MatchSetup, MoveOrder, Owner, Phase, Pose, PublishSet, RunConfig,
    SimBuilder, SimFeature, SimTick, UnitType, build_sim_app, system_order,
};
use fl_test_utils::base_content;

fn id_of<C: Component>(world: &World) -> ComponentId {
    world
        .component_id::<C>()
        .unwrap_or_else(|| panic!("{} is registered", std::any::type_name::<C>()))
}

/// What [`late_writers`] found.
struct Findings {
    /// The index of the snapshot publisher in the executed order.
    publisher: usize,
    /// `(index, "<label> <system>: <what>")` per problem, in executed order.
    found: Vec<(usize, String)>,
}

/// The problems that let a tick's state change after its snapshot was taken:
/// - any `Publish`-phase system (outside its sync point), wherever it sits, that queues
///   deferred commands: the `Publish` sync point applies them after the snapshot;
/// - any system after the snapshot publisher (`publish_snapshot`) that writes a component the
///   snapshot's records are built from, or takes the whole world.
fn late_writers(features: &[Arc<dyn SimFeature>]) -> Findings {
    let mut app = build_sim_app(
        features,
        base_content(),
        MatchSetup::default(),
        RunConfig::default(),
    )
    .expect("the sim builds");
    let lines: Vec<String> = system_order(&mut app).lines().map(str::to_owned).collect();

    let world = app.world_mut();
    let published = [
        ("Pose", id_of::<Pose>(world)),
        ("Owner", id_of::<Owner>(world)),
        ("UnitType", id_of::<UnitType>(world)),
        ("Health", id_of::<Health>(world)),
        ("MoveOrder", id_of::<MoveOrder>(world)),
        ("Invulnerable", id_of::<Invulnerable>(world)),
    ];
    world.schedule_scope(SimTick, |_, schedule| {
        let systems: Vec<_> = schedule
            .systems_with_access()
            .expect("SimTick is initialized")
            .map(|(_, system)| system)
            .collect();
        assert_eq!(
            systems.len(),
            lines.len(),
            "system_order lists every system"
        );
        for (system, line) in systems.iter().zip(&lines) {
            let name = system.name().to_string();
            assert!(
                line.ends_with(&format!(" {name}")),
                "system_order line '{line}' is not system {name}"
            );
        }
        let publisher = systems
            .iter()
            .position(|system| system.name().to_string().ends_with("::publish_snapshot"))
            .expect("the snapshot publisher");
        assert!(
            lines[publisher].starts_with("Publish/Snapshot "),
            "the publisher runs in Publish/Snapshot: {}",
            lines[publisher]
        );
        // The Publish phase starts after the Lifecycle sync point.
        let publish_start = lines
            .iter()
            .position(|line| line.starts_with("Lifecycle/Sync "))
            .expect("the Lifecycle sync point")
            + 1;

        let mut found = Vec::new();
        for (index, (system, line)) in systems.iter().zip(&lines).enumerate().skip(publish_start) {
            if line.starts_with("Publish/Sync ") {
                continue;
            }
            if system.has_deferred() {
                found.push((index, format!("{line}: queues commands")));
            }
            if index <= publisher {
                continue;
            }
            if system.access().is_exclusive() {
                found.push((index, format!("{line}: takes the whole world")));
                continue;
            }
            let access = system.access().to_filtered_access_set();
            for (component, id) in published {
                if access.combined_access().has_write(id) {
                    found.push((index, format!("{line}: writes {component}")));
                }
            }
        }
        Findings { publisher, found }
    })
}

#[test]
fn smoke_nothing_writes_published_state_after_the_snapshot() {
    let findings = late_writers(&fl_catalog::features());
    assert!(
        findings.found.is_empty(),
        "changed after the snapshot: {:#?}",
        findings.found
    );
}

/// Writes `Pose` in `PublishSet::Events`, after the snapshot.
struct LateMover;

impl SimFeature for LateMover {
    fn id(&self) -> &'static str {
        "late_mover"
    }

    fn version(&self) -> u32 {
        1
    }

    fn build(&self, sim: &mut SimBuilder) {
        sim.add_systems(PublishSet::Events, |_poses: Query<&mut Pose>| {});
    }
}

#[test]
fn a_late_writer_is_found() {
    let mut features = fl_catalog::features();
    features.push(Arc::new(LateMover));
    let found = late_writers(&features).found;
    assert_eq!(found.len(), 1, "{found:#?}");
    let (_, problem) = &found[0];
    assert!(
        problem.starts_with("Publish/Events ") && problem.ends_with(": writes Pose"),
        "{found:#?}"
    );
}

/// Queues (empty) commands in the `Publish` phase, before the snapshot publisher. The hook
/// slots are private, so the system goes in the phase itself, ordered before
/// `PublishSet::Snapshot`.
struct EarlyQueuer;

impl SimFeature for EarlyQueuer {
    fn id(&self) -> &'static str {
        "early_queuer"
    }

    fn version(&self) -> u32 {
        1
    }

    fn build(&self, sim: &mut SimBuilder) {
        sim.app_mut().add_systems(
            SimTick,
            (|_commands: Commands| {})
                .in_set(Phase::Publish)
                .before(PublishSet::Snapshot),
        );
    }
}

#[test]
fn deferred_commands_before_the_publisher_are_found() {
    let mut features = fl_catalog::features();
    features.push(Arc::new(EarlyQueuer));
    let findings = late_writers(&features);
    let found = &findings.found;
    assert_eq!(found.len(), 1, "{found:#?}");
    let (index, problem) = &found[0];
    assert!(*index < findings.publisher, "{found:#?}");
    assert!(problem.ends_with(": queues commands"), "{found:#?}");
}
