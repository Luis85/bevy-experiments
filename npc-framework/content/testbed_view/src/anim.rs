//! Actor animation: one `AnimationGraph` per model (built from its named
//! clips), cross-faded whenever the pose read from the sim changes. The pose
//! comes from sim state only: riding (`MountedIn`), the running interaction
//! (`Interacting` verb), crafting (`Crafting`), walking (the position
//! changed during the latest tick), else idle.

use std::time::Duration;

use bevy::{gltf::Gltf, prelude::*, world_serialization::WorldInstanceReady};
use npc_framework::crafting::Crafting;
use npc_framework::interaction::Interacting;
use npc_framework::interaction::builtin::{PICK_UP, USE_TOOL_ON};
use npc_framework::prelude::MountedIn;
use npc_framework::view_helpers::{ViewSystems, VisualOf};

use crate::assets::{ModelKind, ViewAssets};
use crate::visuals::{ActorModel, ActorVisual};

const CROSS_FADE: Duration = Duration::from_millis(200);
/// How long a new pose must hold (real seconds) before it plays; the sim is
/// idle for single ticks between branches.
const DEBOUNCE_SECS: f32 = 0.15;

/// What an actor is doing, as far as its animation is concerned.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Pose {
    #[default]
    Idle,
    Walk,
    /// Chopping, mining (a tool on a target).
    Work,
    /// Doors, mounting, studying, other verbs.
    Interact,
    Craft,
    PickUp,
    Ride,
}

impl Pose {
    const ALL: [Pose; 7] = [
        Pose::Idle,
        Pose::Walk,
        Pose::Work,
        Pose::Interact,
        Pose::Craft,
        Pose::PickUp,
        Pose::Ride,
    ];

    fn slot(self) -> usize {
        self as usize
    }

    fn clip(self, kind: ModelKind) -> &'static str {
        let skeleton = matches!(kind, ModelKind::SkeletonMinion | ModelKind::SkeletonWarrior);
        match self {
            Pose::Idle => "Idle",
            Pose::Walk if skeleton => "Walking_D_Skeletons",
            Pose::Walk => "Walking_A",
            Pose::Work => "1H_Melee_Attack_Chop",
            Pose::Interact => "Interact",
            Pose::Craft => "Use_Item",
            Pose::PickUp => "PickUp",
            Pose::Ride => "Sit_Chair_Idle",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Pose::Idle => "idle",
            Pose::Walk => "walk",
            Pose::Work => "work",
            Pose::Interact => "interact",
            Pose::Craft => "craft",
            Pose::PickUp => "pick up",
            Pose::Ride => "ride",
        }
    }
}

/// The pose for an actor's sim state.
pub fn pose_of(
    mounted: Option<&MountedIn>,
    interacting: Option<&Interacting>,
    crafting: Option<&Crafting>,
    moving: bool,
) -> Pose {
    if mounted.is_some() {
        return Pose::Ride;
    }
    if let Some(i) = interacting {
        return match i.verb.as_str() {
            USE_TOOL_ON => Pose::Work,
            PICK_UP => Pose::PickUp,
            _ => Pose::Interact,
        };
    }
    if crafting.is_some() {
        return Pose::Craft;
    }
    if moving { Pose::Walk } else { Pose::Idle }
}

/// The shared graphs, one per model kind, built once its glb is loaded.
#[derive(Resource, Default)]
pub(crate) struct ActorAnimations {
    graphs: [Option<(Handle<AnimationGraph>, [AnimationNodeIndex; 7])>; 4],
    unavailable: [bool; 4],
}

/// On the entity holding an actor's `AnimationPlayer`.
#[derive(Component, Clone, Copy, Debug)]
pub(crate) struct ActorAnimTarget {
    root: Entity,
    kind: ModelKind,
    current: Option<Pose>,
    /// When the sim first reported a different pose than the one playing.
    pending_since: Option<(Pose, f32)>,
}

/// Meshes of the KayKit kits an actor should not carry around.
const HIDDEN_MESHES: [&str; 12] = [
    "1H_Sword",
    "1H_Sword_Offhand",
    "2H_Sword",
    "Badge_Shield",
    "Rectangle_Shield",
    "Round_Shield",
    "Spike_Shield",
    "Knife",
    "Knife_Offhand",
    "1H_Crossbow",
    "2H_Crossbow",
    "Throwable",
];

pub(crate) fn plugin(app: &mut App) {
    app.init_resource::<ActorAnimations>().add_systems(
        Update,
        (build_graphs, drive_animation)
            .chain()
            .in_set(ViewSystems::Animate),
    );
}

fn build_graphs(
    assets: Option<Res<ViewAssets>>,
    gltfs: Res<Assets<Gltf>>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
    mut anims: ResMut<ActorAnimations>,
) {
    let Some(assets) = assets else { return };
    for kind in ModelKind::ALL {
        let i = kind.index();
        if anims.graphs[i].is_some() || anims.unavailable[i] {
            continue;
        }
        let Some(gltf) = gltfs.get(&assets.actor_gltf[i]) else {
            continue;
        };
        let Some(idle) = gltf.named_animations.get("Idle").cloned() else {
            warn!("testbed_view: {kind:?} has no `Idle` clip; animation disabled");
            anims.unavailable[i] = true;
            continue;
        };
        let mut graph = AnimationGraph::new();
        let root = graph.root;
        let nodes = Pose::ALL.map(|pose| {
            let name = pose.clip(kind);
            let clip = gltf.named_animations.get(name).cloned().unwrap_or_else(|| {
                warn!("testbed_view: {kind:?} has no `{name}` clip; using Idle");
                idle.clone()
            });
            graph.add_clip(clip, 1.0, root)
        });
        anims.graphs[i] = Some((graphs.add(graph), nodes));
    }
}

/// Observer on an actor model: find its `AnimationPlayer` and hand slots,
/// hide the kit's weapons.
pub(crate) fn on_actor_ready(
    ready: On<WorldInstanceReady>,
    mut commands: Commands,
    models: Query<&ActorModel>,
    children: Query<&Children>,
    players: Query<(), With<AnimationPlayer>>,
    names: Query<&Name>,
    mut actors: Query<&mut ActorVisual>,
) {
    let Ok(model) = models.get(ready.entity) else {
        return;
    };
    let Ok(mut actor) = actors.get_mut(model.root) else {
        return;
    };
    for e in children.iter_descendants(ready.entity) {
        if players.contains(e) {
            commands.entity(e).insert(ActorAnimTarget {
                root: model.root,
                kind: actor.kind,
                current: None,
                pending_since: None,
            });
        }
        let Ok(name) = names.get(e) else { continue };
        match name.as_str() {
            "handslot.r" => actor.hand_r = Some(e),
            "handslot.l" => actor.hand_l = Some(e),
            n if HIDDEN_MESHES.contains(&n) => {
                commands.entity(e).insert(Visibility::Hidden);
            }
            _ => {}
        }
    }
}

#[allow(clippy::type_complexity)]
fn drive_animation(
    mut commands: Commands,
    time: Res<Time>,
    anims: Res<ActorAnimations>,
    roots: Query<(&VisualOf, &ActorVisual)>,
    sims: Query<(Option<&MountedIn>, Option<&Interacting>, Option<&Crafting>)>,
    mut players: Query<(
        Entity,
        &mut AnimationPlayer,
        &mut ActorAnimTarget,
        Option<&mut AnimationTransitions>,
    )>,
) {
    let now = time.elapsed_secs();
    for (entity, mut player, mut target, transitions) in &mut players {
        let Some((graph, nodes)) = &anims.graphs[target.kind.index()] else {
            continue;
        };
        let pose = roots
            .get(target.root)
            .ok()
            .and_then(|(of, actor)| {
                let (m, i, c) = sims.get(of.0).ok()?;
                Some(pose_of(m, i, c, actor.motion.moving))
            })
            .unwrap_or_default();
        if target.current.is_some() && target.current != Some(pose) {
            let since = match target.pending_since {
                Some((p, t)) if p == pose => t,
                _ => {
                    target.pending_since = Some((pose, now));
                    now
                }
            };
            // Ride and walk start at once; idle (and short blips) wait.
            let urgent = matches!(pose, Pose::Ride | Pose::Walk | Pose::Work);
            if !urgent && now - since < DEBOUNCE_SECS {
                continue;
            }
        }
        target.pending_since = None;
        let node = nodes[pose.slot()];
        match transitions {
            None => {
                let mut transitions = AnimationTransitions::new();
                transitions.play(&mut player, node, Duration::ZERO).repeat();
                commands
                    .entity(entity)
                    .insert((AnimationGraphHandle(graph.clone()), transitions));
                target.current = Some(pose);
            }
            Some(mut transitions) if target.current != Some(pose) => {
                transitions.play(&mut player, node, CROSS_FADE).repeat();
                target.current = Some(pose);
            }
            Some(_) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use npc_framework::interaction::Source;
    use npc_framework::prelude::SimId;

    use super::*;

    fn doing(verb: &str) -> Interacting {
        Interacting {
            verb: verb.into(),
            target: SimId(1),
            item: None,
            started: 0,
            ends: 10,
            source: Source::default(),
        }
    }

    #[test]
    fn poses_follow_sim_state_by_priority() {
        let ride = MountedIn {
            vehicle: SimId(3),
            seat: 0,
        };
        assert_eq!(
            pose_of(Some(&ride), Some(&doing("open")), None, true),
            Pose::Ride
        );
        assert_eq!(
            pose_of(None, Some(&doing(USE_TOOL_ON)), None, true),
            Pose::Work
        );
        assert_eq!(
            pose_of(None, Some(&doing(PICK_UP)), None, false),
            Pose::PickUp
        );
        assert_eq!(
            pose_of(None, Some(&doing("unlock")), None, false),
            Pose::Interact
        );
        assert_eq!(pose_of(None, None, None, true), Pose::Walk);
        assert_eq!(pose_of(None, None, None, false), Pose::Idle);
    }

    #[test]
    fn skeletons_shamble() {
        assert_eq!(
            Pose::Walk.clip(ModelKind::SkeletonMinion),
            "Walking_D_Skeletons"
        );
        assert_eq!(Pose::Walk.clip(ModelKind::Knight), "Walking_A");
        assert_eq!(Pose::ALL.len(), 7);
    }
}
