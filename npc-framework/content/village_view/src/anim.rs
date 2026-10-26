//! Villager animation: an `AnimationGraph` built from the glb's named clips,
//! attached to the `AnimationPlayer` of each spawned villager model, with
//! cross-fades whenever the sim `Activity` changes.

use std::time::Duration;

use bevy::{gltf::Gltf, prelude::*, world_serialization::WorldInstanceReady};
use npc_framework::view_helpers::{ViewSystems, VisualOf};
use village_content::Activity;

use crate::assets::ViewAssets;

const CROSS_FADE: Duration = Duration::from_millis(200);

/// Clip per `Activity`, in `activity_slot` order.
const CLIPS: [&str; 5] = ["Idle", "Walking_A", "Interact", "Use_Item", "Lie_Idle"];

/// Meshes of the Rogue's kit that a villager should not carry around.
const HIDDEN_MESHES: [&str; 5] = [
    "Knife",
    "Knife_Offhand",
    "1H_Crossbow",
    "2H_Crossbow",
    "Throwable",
];

fn activity_slot(activity: Activity) -> usize {
    match activity {
        Activity::Idle => 0,
        Activity::Walk => 1,
        Activity::Work => 2,
        // Drinking at the well reuses the eating clip.
        Activity::Eat | Activity::Drink => 3,
        Activity::Sleep => 4,
    }
}

/// The shared villager graph, built once the glb is loaded.
#[derive(Resource, Default)]
pub(crate) struct VillagerAnimations {
    graph: Option<(Handle<AnimationGraph>, [AnimationNodeIndex; 5])>,
    /// Set when the glb failed to provide even `Idle`; stops retrying.
    unavailable: bool,
}

/// On the villager model entity (the `WorldAssetRoot` child of the visual root).
#[derive(Component, Clone, Copy, Debug)]
pub(crate) struct VillagerModel {
    pub root: Entity,
}

/// On the entity holding the villager's `AnimationPlayer`.
#[derive(Component, Clone, Copy, Debug)]
pub(crate) struct VillagerAnimTarget {
    root: Entity,
    current: Option<Activity>,
    /// When the sim first reported `Idle` while another clip was playing.
    /// Between two branches the sim is `Idle` for a single tick; this keeps
    /// that blip from cross-fading Work → Idle → Work.
    idle_since: Option<f32>,
}

/// How long `Idle` must persist (real seconds) before the idle clip plays.
const IDLE_DEBOUNCE_SECS: f32 = 0.15;

pub(crate) fn plugin(app: &mut App) {
    app.init_resource::<VillagerAnimations>().add_systems(
        Update,
        (build_graph, drive_animation)
            .chain()
            .in_set(ViewSystems::Animate),
    );
}

fn build_graph(
    assets: Option<Res<ViewAssets>>,
    gltfs: Res<Assets<Gltf>>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
    mut anims: ResMut<VillagerAnimations>,
) {
    if anims.graph.is_some() || anims.unavailable {
        return;
    }
    let Some(assets) = assets else { return };
    let Some(gltf) = gltfs.get(&assets.villager_gltf) else {
        return;
    };
    let Some(idle) = gltf.named_animations.get("Idle").cloned() else {
        warn!("village_view: villager glb has no `Idle` clip; animation disabled");
        anims.unavailable = true;
        return;
    };
    let mut graph = AnimationGraph::new();
    let root = graph.root;
    let nodes = CLIPS.map(|name| {
        let clip = gltf.named_animations.get(name).cloned().unwrap_or_else(|| {
            warn!("village_view: villager glb has no `{name}` clip; using Idle");
            idle.clone()
        });
        graph.add_clip(clip, 1.0, root)
    });
    anims.graph = Some((graphs.add(graph), nodes));
}

/// Observer on the villager model: find the `AnimationPlayer` and hide weapons.
pub(crate) fn on_villager_ready(
    ready: On<WorldInstanceReady>,
    mut commands: Commands,
    models: Query<&VillagerModel>,
    children: Query<&Children>,
    players: Query<(), With<AnimationPlayer>>,
    names: Query<&Name>,
) {
    let Ok(model) = models.get(ready.entity) else {
        return;
    };
    for e in children.iter_descendants(ready.entity) {
        if players.contains(e) {
            commands.entity(e).insert(VillagerAnimTarget {
                root: model.root,
                current: None,
                idle_since: None,
            });
        }
        if names
            .get(e)
            .is_ok_and(|n| HIDDEN_MESHES.contains(&n.as_str()))
        {
            commands.entity(e).insert(Visibility::Hidden);
        }
    }
}

fn drive_animation(
    mut commands: Commands,
    time: Res<Time>,
    anims: Res<VillagerAnimations>,
    roots: Query<&VisualOf>,
    activities: Query<&Activity>,
    mut players: Query<(
        Entity,
        &mut AnimationPlayer,
        &mut VillagerAnimTarget,
        Option<&mut AnimationTransitions>,
    )>,
) {
    let Some((graph, nodes)) = &anims.graph else {
        return;
    };
    for (entity, mut player, mut target, transitions) in &mut players {
        let activity = roots
            .get(target.root)
            .ok()
            .and_then(|of| activities.get(of.0).ok())
            .copied()
            .unwrap_or_default();
        if activity == Activity::Idle && target.current.is_some_and(|c| c != Activity::Idle) {
            let since = *target.idle_since.get_or_insert(time.elapsed_secs());
            if time.elapsed_secs() - since < IDLE_DEBOUNCE_SECS {
                continue;
            }
        }
        target.idle_since = None;
        let node = nodes[activity_slot(activity)];
        match transitions {
            None => {
                let mut transitions = AnimationTransitions::new();
                transitions.play(&mut player, node, Duration::ZERO).repeat();
                commands
                    .entity(entity)
                    .insert((AnimationGraphHandle(graph.clone()), transitions));
                target.current = Some(activity);
            }
            Some(mut transitions) if target.current != Some(activity) => {
                transitions.play(&mut player, node, CROSS_FADE).repeat();
                target.current = Some(activity);
            }
            Some(_) => {}
        }
    }
}
