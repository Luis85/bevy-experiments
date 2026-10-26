//! Every sim component and resource is classified (ADR-0006).

use std::path::PathBuf;
use std::sync::Arc;

use bevy_ecs::component::Component;
use bevy_ecs::hierarchy::{ChildOf, Children};
use bevy_ecs::prelude::{Add, Commands, On, Res};
use fl_math::{Angle, FxVec2};
use fl_protocol::{Tick, tables};
use fl_sim::{
    BuildError, Class, MatchHeaderRes, MatchSetup, Pose, RunConfig, SimBuilder, SimClock,
    SimFeature, ThinkSet, build_sim_app, check_classification, load_content, sim_registry,
    step_tick,
};
use fl_view::TickInput;

fn core_mod() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/core_mod")
}

fn build(features: &[Arc<dyn SimFeature>]) -> bevy_app::App {
    let content = load_content(features, &[core_mod()]).expect("core_mod loads");
    build_sim_app(
        features,
        content,
        MatchSetup::default(),
        RunConfig::default(),
    )
    .expect("the sim builds")
}

#[test]
fn core_app_is_fully_classified() {
    let app = build(&[]);
    assert_eq!(check_classification(app.world()), Ok(()));
}

#[derive(Component)]
struct Tag;

struct ExternalComponentFeature;

impl SimFeature for ExternalComponentFeature {
    fn id(&self) -> &'static str {
        "test"
    }

    fn version(&self) -> u32 {
        1
    }

    fn build(&self, sim: &mut SimBuilder) {
        sim.register_component::<Tag>("test.tag", Class::External);
    }
}

#[test]
#[should_panic(expected = "External is for resources")]
fn external_component_panics() {
    let _ = sim_registry(&[Arc::new(ExternalComponentFeature)]);
}

#[test]
fn classified_names_in_tables() {
    let app = build(&[]);
    let header = &app.world().resource::<MatchHeaderRes>().0;
    let components = header
        .string_tables
        .get(tables::SIM_COMPONENTS)
        .expect("sim_components table");
    let resources = header
        .string_tables
        .get(tables::SIM_RESOURCES)
        .expect("sim_resources table");
    assert!(
        components.iter().any(|n| n == "core.pose"),
        "{components:?}"
    );
    assert!(
        resources.iter().any(|n| n == "core.players"),
        "{resources:?}"
    );
    assert!(components.is_sorted() && resources.is_sorted());
    assert!(
        !components.iter().any(|n| n == "core.players"),
        "resources are not components"
    );
}

/// Classifies `Tag` as `"<id>.tag"`.
struct TagFeature(&'static str, &'static str);

impl SimFeature for TagFeature {
    fn id(&self) -> &'static str {
        self.0
    }

    fn version(&self) -> u32 {
        1
    }

    fn build(&self, sim: &mut SimBuilder) {
        sim.register_component::<Tag>(self.1, Class::Transient);
    }
}

#[test]
#[should_panic(expected = "as 'a.tag' by 'a' and as 'b.tag' by 'b'")]
fn classifying_a_type_twice_names_both_features() {
    let _ = sim_registry(&[
        Arc::new(TagFeature("a", "a.tag")),
        Arc::new(TagFeature("b", "b.tag")),
    ]);
}

#[derive(bevy_ecs::message::Message)]
struct Hit;

/// Adds a message buffer around the builder, through Bevy's own `add_message`: never
/// classified, never cleared.
struct BypassMessageFeature;

impl SimFeature for BypassMessageFeature {
    fn id(&self) -> &'static str {
        "bypass"
    }

    fn version(&self) -> u32 {
        1
    }

    fn build(&self, sim: &mut SimBuilder) {
        sim.app_mut().add_message::<Hit>();
    }
}

fn unclassified(features: &[Arc<dyn SimFeature>]) -> Vec<String> {
    let content = load_content(features, &[core_mod()]).expect("core_mod loads");
    match build_sim_app(
        features,
        content,
        MatchSetup::default(),
        RunConfig::default(),
    ) {
        Err(BuildError::Unclassified(names)) => names,
        Err(other) => panic!("expected Unclassified, got {other}"),
        Ok(_) => panic!("expected Unclassified, the sim built"),
    }
}

#[cfg(debug_assertions)]
#[test]
fn message_buffer_added_around_the_builder_is_unclassified() {
    let names = unclassified(&[Arc::new(BypassMessageFeature)]);
    assert!(
        names.iter().any(|n| n.ends_with("Messages<classify::Hit>")),
        "{names:?}"
    );
}

/// Adds an observer to the sim world.
struct ObserverFeature;

impl SimFeature for ObserverFeature {
    fn id(&self) -> &'static str {
        "observer"
    }

    fn version(&self) -> u32 {
        1
    }

    fn build(&self, sim: &mut SimBuilder) {
        sim.app_mut().add_observer(|_: On<Add<Pose>>| {});
    }
}

#[cfg(debug_assertions)]
#[test]
fn observers_are_not_allowed_in_the_sim_world() {
    let names = unclassified(&[Arc::new(ObserverFeature)]);
    assert!(
        names
            .iter()
            .any(|n| n.contains("Observer (not allowed in the sim world)")),
        "{names:?}"
    );
}

#[test]
fn relationships_are_not_allowed_in_the_sim_world() {
    let mut app = build(&[]);
    let world = app.world_mut();
    let parent = world.spawn_empty().id();
    world.spawn(ChildOf(parent));
    let names = check_classification(world).expect_err("ChildOf and Children are forbidden");
    assert_eq!(
        names,
        [
            "bevy_ecs::hierarchy::ChildOf (not allowed in the sim world)",
            "bevy_ecs::hierarchy::Children (not allowed in the sim world)",
        ]
    );
}

/// Spawns a `Pose` without a `SimEntity` through `Commands` in tick 1: around the funnel.
struct StraySpawnFeature;

impl SimFeature for StraySpawnFeature {
    fn id(&self) -> &'static str {
        "stray_spawn"
    }

    fn version(&self) -> u32 {
        1
    }

    fn build(&self, sim: &mut SimBuilder) {
        sim.add_systems(
            ThinkSet::Orders,
            |clock: Res<SimClock>, mut commands: Commands| {
                if clock.tick == Tick(1) {
                    commands.spawn(Pose {
                        pos: FxVec2::ZERO,
                        heading: Angle::ZERO,
                    });
                }
            },
        );
    }
}

#[cfg(debug_assertions)]
#[test]
#[should_panic(expected = "1 entities with core.pose have no SimEntity")]
fn entity_spawned_around_the_funnel_fails_its_tick() {
    let mut app = build(&[Arc::new(StraySpawnFeature)]);
    for tick in 0..2 {
        let input = TickInput {
            tick: Tick(tick),
            commands: Vec::new(),
        };
        step_tick(app.world_mut(), input);
    }
}

#[derive(bevy_ecs::resource::Resource, Default)]
struct PerTick(u32);

#[derive(bevy_ecs::resource::Resource, Default)]
struct Kept(u32);

/// Adds 1 to `PerTick` (reset every tick) and `Kept` (kept, with a reason) every tick.
struct TransientResourceFeature;

impl SimFeature for TransientResourceFeature {
    fn id(&self) -> &'static str {
        "per_tick"
    }

    fn version(&self) -> u32 {
        1
    }

    fn build(&self, sim: &mut SimBuilder) {
        sim.app_mut()
            .init_resource::<PerTick>()
            .init_resource::<Kept>();
        sim.register_resource::<PerTick>("per_tick.count", Class::transient())
            .register_resource::<Kept>(
                "per_tick.kept",
                Class::TransientKept("a test of the opt-out"),
            )
            .add_systems(
                ThinkSet::Orders,
                |mut per_tick: bevy_ecs::system::ResMut<PerTick>,
                 mut kept: bevy_ecs::system::ResMut<Kept>| {
                    per_tick.0 += 1;
                    kept.0 += 1;
                },
            );
    }
}

#[test]
fn transient_resources_reset_every_tick_unless_kept() {
    let mut app = build(&[Arc::new(TransientResourceFeature)]);
    for tick in 0..3 {
        let input = TickInput {
            tick: Tick(tick),
            commands: Vec::new(),
        };
        step_tick(app.world_mut(), input);
        assert_eq!(app.world().resource::<PerTick>().0, 1, "only this tick's");
    }
    assert_eq!(app.world().resource::<Kept>().0, 3, "kept: never reset");
}

/// Classifies `PerTick` plain `Transient`, which a resource cannot be.
struct PlainTransientResourceFeature;

impl SimFeature for PlainTransientResourceFeature {
    fn id(&self) -> &'static str {
        "plain"
    }

    fn version(&self) -> u32 {
        1
    }

    fn build(&self, sim: &mut SimBuilder) {
        sim.register_resource::<PerTick>("plain.count", Class::Transient);
    }
}

#[test]
#[should_panic(expected = "cannot be plain Transient")]
fn plain_transient_resource_panics() {
    let _ = sim_registry(&[Arc::new(PlainTransientResourceFeature)]);
}

/// Classifies the component `Tag` with a reset, which only a resource can have.
struct ResetComponentFeature;

impl SimFeature for ResetComponentFeature {
    fn id(&self) -> &'static str {
        "reset"
    }

    fn version(&self) -> u32 {
        1
    }

    fn build(&self, sim: &mut SimBuilder) {
        sim.register_component::<Tag>("reset.tag", Class::TransientKept("no"));
    }
}

#[test]
#[should_panic(expected = "a Transient component is removed every tick")]
fn kept_transient_component_panics() {
    let _ = sim_registry(&[Arc::new(ResetComponentFeature)]);
}

/// Queries `ChildOf` and `Children` (so both are registered at build) and spawns a child in
/// tick 1: a forbidden component first instanced mid-match, with no new type registered.
struct LateChildFeature;

impl SimFeature for LateChildFeature {
    fn id(&self) -> &'static str {
        "late_child"
    }

    fn version(&self) -> u32 {
        1
    }

    fn build(&self, sim: &mut SimBuilder) {
        sim.add_systems(
            ThinkSet::Orders,
            |clock: Res<SimClock>,
             _related: bevy_ecs::system::Query<(Option<&ChildOf>, Option<&Children>)>,
             mut commands: Commands| {
                if clock.tick == Tick(1) {
                    let parent = commands.spawn_empty().id();
                    commands.spawn(ChildOf(parent));
                }
            },
        );
    }
}

#[cfg(debug_assertions)]
#[test]
#[should_panic(expected = "after tick 1: [\"bevy_ecs::hierarchy::ChildOf (not allowed")]
fn forbidden_component_instanced_mid_match_fails_its_tick() {
    let mut app = build(&[Arc::new(LateChildFeature)]);
    for tick in 0..2 {
        let input = TickInput {
            tick: Tick(tick),
            commands: Vec::new(),
        };
        step_tick(app.world_mut(), input);
    }
}
