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
    /// Island life (`island_poses`): resting through the night.
    Sleep,
    /// A daytime rest.
    Sit,
    /// Walking with bulk goods.
    Carry,
    /// Felling timber.
    Chop,
    /// Hoeing, planting, harvesting, foraging.
    Farm,
    Fish,
    /// Smithing, carpentry, building.
    Hammer,
}

impl Pose {
    pub(crate) const COUNT: usize = 14;
    pub(crate) const ALL: [Pose; Pose::COUNT] = [
        Pose::Idle,
        Pose::Walk,
        Pose::Work,
        Pose::Interact,
        Pose::Craft,
        Pose::PickUp,
        Pose::Ride,
        Pose::Sleep,
        Pose::Sit,
        Pose::Carry,
        Pose::Chop,
        Pose::Farm,
        Pose::Fish,
        Pose::Hammer,
    ];

    fn slot(self) -> usize {
        self as usize
    }

    /// Every name below exists in the shipped rigs (see
    /// `docs/island-atmosphere.md` for the inventory). Where a rig has no
    /// dedicated clip the closest one is used; nothing is retargeted.
    pub(crate) fn clip(self, kind: ModelKind) -> &'static str {
        if kind.civilian_rig() {
            // The Quaternius civilians ship 24 clips with no sit, lie, carry
            // or fishing motion: those fall back to a calm stance or the walk.
            return match self {
                Pose::Idle | Pose::Ride => "CharacterArmature|Idle",
                Pose::Walk | Pose::Carry => "CharacterArmature|Walk",
                Pose::Work | Pose::Craft | Pose::PickUp | Pose::Farm => {
                    "CharacterArmature|Interact"
                }
                Pose::Interact => "CharacterArmature|Wave",
                Pose::Sleep | Pose::Sit => "CharacterArmature|Idle_Neutral",
                Pose::Chop => "CharacterArmature|Sword_Slash",
                Pose::Hammer => "CharacterArmature|Punch_Right",
                Pose::Fish => "CharacterArmature|Idle_Sword",
            };
        }
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
            Pose::Sleep => "Lie_Idle",
            Pose::Sit => "Sit_Floor_Idle",
            // No carry clip in the KayKit set: the slower second walk.
            Pose::Carry => "Walking_B",
            Pose::Chop => "1H_Melee_Attack_Slice_Diagonal",
            Pose::Farm => "2H_Melee_Attack_Chop",
            // No fishing clip: the two-handed ready stance holds the rod.
            Pose::Fish => "2H_Melee_Idle",
            Pose::Hammer => "1H_Melee_Attack_Chop",
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
            Pose::Sleep => "sleep",
            Pose::Sit => "sit",
            Pose::Carry => "carry",
            Pose::Chop => "chop",
            Pose::Farm => "farm",
            Pose::Fish => "fish",
            Pose::Hammer => "hammer",
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
    graphs: Vec<Option<(Handle<AnimationGraph>, [AnimationNodeIndex; Pose::COUNT])>>,
    unavailable: Vec<bool>,
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
pub(crate) const HIDDEN_MESHES: &[&str] = &[
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
    "1H_Axe",
    "1H_Axe_Offhand",
    "2H_Axe",
    "Staff",
    "Wand",
    "Barbarian_Round_Shield",
    "Mug",
    "Spellbook",
    "Spellbook_open",
    "1H_Wand",
    "2H_Staff",
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
    anims.graphs.resize_with(ModelKind::ANIMATED.len(), || None);
    anims.unavailable.resize(ModelKind::ANIMATED.len(), false);
    for kind in ModelKind::ANIMATED {
        let i = kind.index();
        if anims.graphs[i].is_some() || anims.unavailable[i] {
            continue;
        }
        let Some(gltf) = gltfs.get(&assets.actor_gltf[i]) else {
            continue;
        };
        let Some(idle) = gltf.named_animations.get(kind.idle_clip()).cloned() else {
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
            "Wrist.R" | "Wrist.L" if actor.kind.civilian_rig() => {
                // Imported FBX-to-glTF bones inherit a scale of 100. Normalize
                // the socket to the existing item-in-hand V convention.
                let socket = commands
                    .spawn((
                        Name::new("civilian tool socket"),
                        ChildOf(e),
                        Transform::from_scale(Vec3::splat(crate::visuals::V / 100.0)),
                        Visibility::default(),
                    ))
                    .id();
                if name.as_str() == "Wrist.R" {
                    actor.hand_r = Some(socket);
                } else {
                    actor.hand_l = Some(socket);
                }
            }
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
        let Some((graph, nodes)) = anims
            .graphs
            .get(target.kind.index())
            .and_then(Option::as_ref)
        else {
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
            let urgent = matches!(
                pose,
                Pose::Ride
                    | Pose::Walk
                    | Pose::Work
                    | Pose::Carry
                    | Pose::Chop
                    | Pose::Farm
                    | Pose::Fish
                    | Pose::Hammer
            );
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

    #[test]
    fn civilian_semantic_clips_use_the_imported_rig_names() {
        assert_eq!(
            Pose::Walk.clip(ModelKind::VillageWoman),
            "CharacterArmature|Walk"
        );
        assert_eq!(
            Pose::Work.clip(ModelKind::FarmerMan),
            "CharacterArmature|Interact"
        );
        assert_eq!(
            Pose::Interact.clip(ModelKind::TravellerWoman),
            "CharacterArmature|Wave"
        );
    }

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
        assert_eq!(Pose::ALL.len(), Pose::COUNT);
        for (slot, pose) in Pose::ALL.into_iter().enumerate() {
            assert_eq!(pose.slot(), slot);
        }
    }

    /// Clip names read from the shipped GLBs (see `docs/island-atmosphere.md`).
    const KAYKIT_CLIPS: &[&str] = &[
        "1H_Melee_Attack_Chop",
        "1H_Melee_Attack_Slice_Diagonal",
        "2H_Melee_Attack_Chop",
        "2H_Melee_Idle",
        "Idle",
        "Interact",
        "Lie_Idle",
        "PickUp",
        "Sit_Chair_Idle",
        "Sit_Floor_Idle",
        "Use_Item",
        "Walking_A",
        "Walking_B",
        "Walking_D_Skeletons",
    ];
    const CIVILIAN_CLIPS: &[&str] = &[
        "CharacterArmature|Idle",
        "CharacterArmature|Idle_Neutral",
        "CharacterArmature|Idle_Sword",
        "CharacterArmature|Interact",
        "CharacterArmature|Punch_Right",
        "CharacterArmature|Sword_Slash",
        "CharacterArmature|Walk",
        "CharacterArmature|Wave",
    ];

    #[test]
    fn every_pose_maps_to_a_clip_that_ships_in_its_rig() {
        for kind in ModelKind::ANIMATED {
            let shipped = if kind.civilian_rig() {
                CIVILIAN_CLIPS
            } else {
                KAYKIT_CLIPS
            };
            for pose in Pose::ALL {
                assert!(
                    shipped.contains(&pose.clip(kind)),
                    "{kind:?} {pose:?} -> {}",
                    pose.clip(kind)
                );
            }
        }
    }

    #[test]
    fn island_activities_read_as_distinct_motions_on_the_kaykit_rig() {
        let work = [
            Pose::Sleep,
            Pose::Sit,
            Pose::Carry,
            Pose::Chop,
            Pose::Farm,
            Pose::Fish,
            Pose::Hammer,
        ];
        let clips: std::collections::BTreeSet<_> = work
            .iter()
            .map(|pose| pose.clip(ModelKind::Barbarian))
            .collect();
        assert_eq!(clips.len(), work.len());
        assert!(!clips.contains("Idle") && !clips.contains("Walking_A"));
        // The civilian rig has fewer clips; work motions still differ.
        let civilian: std::collections::BTreeSet<_> =
            [Pose::Chop, Pose::Farm, Pose::Hammer, Pose::Fish]
                .iter()
                .map(|pose| pose.clip(ModelKind::FarmerMan))
                .collect();
        assert_eq!(civilian.len(), 4);
    }
}
