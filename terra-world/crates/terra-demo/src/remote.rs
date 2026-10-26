use bevy::prelude::*;
use std::collections::BTreeMap;
use terra_bevy::{
    GeoActor, GeoAnchor, NetworkStatus, SandboxRuntime, WorldSet,
    render::{LayerVisual, actor_mesh},
};
use terra_core::{ActorKind, Layer};
pub struct RemoteVisualPlugin;
impl Plugin for RemoteVisualPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            sync_players
                .before(WorldSet::Render)
                .after(WorldSet::Sandbox),
        );
    }
}
#[derive(Component)]
struct PlayerVisual;
#[derive(Component)]
struct RemoteActor;
fn sync_players(
    mut commands: Commands,
    net: Option<Res<NetworkStatus>>,
    sandbox: Res<SandboxRuntime>,
    mut remote: Query<(Entity, &mut GeoActor, &mut Mesh3d), With<RemoteActor>>,
    mut player: Query<&mut GeoAnchor, With<PlayerVisual>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut assets: Local<Option<(Vec<Handle<Mesh>>, Handle<StandardMaterial>)>>,
) {
    let (actor_meshes, material) = assets
        .get_or_insert_with(|| {
            (
                ActorKind::ALL
                    .iter()
                    .map(|kind| meshes.add(actor_mesh(*kind)))
                    .collect(),
                materials.add(StandardMaterial {
                    base_color: Color::WHITE,
                    ..default()
                }),
            )
        })
        .clone();
    if let Some(net) = net {
        let existing: BTreeMap<_, _> = remote.iter().map(|(e, a, _)| (a.0.id, e)).collect();
        for (entity, mut actor, mut mesh) in &mut remote {
            if let Some(state) = net.actors.get(&actor.0.id) {
                if actor.0.kind != state.kind {
                    mesh.0 = actor_meshes[state.kind as usize].clone();
                }
                actor.0 = state.clone();
            } else {
                commands.entity(entity).despawn();
            }
        }
        for actor in net.actors.values() {
            if !existing.contains_key(&actor.id) {
                commands.spawn((
                    RemoteActor,
                    GeoActor(actor.clone()),
                    LayerVisual(Layer::Actors),
                    Mesh3d(actor_meshes[actor.kind as usize].clone()),
                    MeshMaterial3d(material.clone()),
                    Transform::default(),
                ));
            }
        }
    } else if let Some(p) = sandbox.state.players.get(&sandbox.player_id) {
        if let Ok(mut anchor) = player.single_mut() {
            anchor.0 = p.position;
        } else {
            commands.spawn((
                PlayerVisual,
                GeoAnchor(p.position),
                LayerVisual(Layer::Actors),
                Mesh3d(actor_meshes[ActorKind::Human as usize].clone()),
                MeshMaterial3d(material),
                Transform::default(),
            ));
        }
    }
}
