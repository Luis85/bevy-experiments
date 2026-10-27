//! Host-side landmark knowledge, projected above the world and every fog mask.
//! These are noninteractive UI entities, never simulation visuals or pick targets.
use std::collections::BTreeMap;
use std::time::Duration;

use bevy::picking::Pickable;
use bevy::prelude::*;
use npc_framework::agent_io::map::{Landmark, MapEnabled};
use npc_framework::agent_io::notes::{NotesDirs, NotesStores};
use npc_framework::prelude::SimPos;
use npc_framework::view_helpers::{ViewSystems, sim_to_world};

use crate::explorer_panel::{ExplorerFiles, map_actor};

const POLL: Duration = Duration::from_secs(1);
const MARK: Color = Color::srgb(0.95, 0.80, 0.45);

#[derive(Resource)]
struct Knowledge {
    timer: Timer,
    folder: Option<std::path::PathBuf>,
    landmarks: Vec<Landmark>,
    enabled: bool,
    dirty: bool,
}
impl Default for Knowledge {
    fn default() -> Self {
        Self {
            timer: Timer::new(POLL, TimerMode::Repeating),
            folder: None,
            landmarks: Vec::new(),
            enabled: false,
            dirty: true,
        }
    }
}
#[derive(Component)]
struct LandmarkLayer;
#[derive(Component, Clone, Debug, PartialEq, Eq)]
struct LandmarkMarker {
    id: u32,
    pos: [i32; 2],
}

#[derive(Component)]
struct LandmarkLabel {
    id: u32,
    fallback_size: Vec2,
}

fn label_size(text: &str) -> Vec2 {
    // The default Fira Mono at 12px. An explicit width avoids shrink-to-fit
    // wrapping against the one-pixel anchor's containing block.
    let text_width = text.chars().count() as f32 * 7.5;
    let width = (text_width + 12.0).clamp(120.0, 240.0);
    Vec2::new(
        width,
        (text_width / (width - 10.0)).ceil().max(1.0) * 16.0 + 6.0,
    )
}
#[derive(Clone, Copy, Debug)]
struct LabelRequest {
    id: u32,
    anchor: Vec2,
    size: Vec2,
}

/// Greedy placement in stable landmark-ID order. Only the label moves;
/// ground anchors are projected separately and never participate in packing.
fn place_labels(mut requests: Vec<LabelRequest>, viewport: Option<Rect>) -> BTreeMap<u32, Rect> {
    const GAP: f32 = 4.0;
    requests.sort_by_key(|request| request.id);
    let mut placed = BTreeMap::<u32, Rect>::new();
    for request in requests {
        let preferred = request.anchor + Vec2::new(9.0, -12.0);
        let left = request.anchor.x - request.size.x - 9.0;
        let mut candidates = vec![preferred, Vec2::new(left, preferred.y)];
        for rect in placed.values() {
            for x in [preferred.x, left] {
                candidates.push(Vec2::new(x, rect.max.y + GAP));
                candidates.push(Vec2::new(x, rect.min.y - request.size.y - GAP));
            }
        }
        if let Some(viewport) = viewport {
            for candidate in &mut candidates {
                candidate.x = candidate.x.clamp(
                    viewport.min.x,
                    (viewport.max.x - request.size.x).max(viewport.min.x),
                );
                candidate.y = candidate.y.clamp(
                    viewport.min.y,
                    (viewport.max.y - request.size.y).max(viewport.min.y),
                );
            }
        }
        // Stable sort retains right/left preference when distances tie.
        candidates.sort_by(|a, b| {
            a.distance_squared(preferred)
                .total_cmp(&b.distance_squared(preferred))
        });
        let clear = |candidate: Vec2| {
            let next = Rect::from_corners(candidate, candidate + request.size);
            placed.values().all(|rect| {
                next.max.x + GAP <= rect.min.x
                    || next.min.x >= rect.max.x + GAP
                    || next.max.y + GAP <= rect.min.y
                    || next.min.y >= rect.max.y + GAP
            })
        };
        let position = candidates
            .into_iter()
            .find(|candidate| clear(*candidate))
            .unwrap_or_else(|| {
                // A fully crowded viewport still gets a nonoverlapping stack;
                // never move or reveal an offscreen ground anchor to make room.
                Vec2::new(
                    preferred.x,
                    placed
                        .values()
                        .map(|rect| rect.max.y + GAP)
                        .fold(preferred.y, f32::max),
                )
            });
        placed.insert(
            request.id,
            Rect::from_corners(position, position + request.size),
        );
    }
    placed
}

pub(crate) fn plugin(app: &mut App) {
    app.init_resource::<Knowledge>()
        .add_systems(Startup, spawn.run_if(resource_exists::<ExplorerFiles>))
        .add_systems(
            Update,
            (poll, rebuild, project)
                .chain()
                .in_set(ViewSystems::Present)
                .run_if(resource_exists::<ExplorerFiles>),
        );
}
fn spawn(mut commands: Commands) {
    commands.spawn((
        LandmarkLayer,
        Name::new("landmark knowledge overlay"),
        Pickable::IGNORE,
        GlobalZIndex(5),
        Node {
            position_type: PositionType::Absolute,
            left: px(0),
            top: px(0),
            width: percent(100),
            height: percent(100),
            ..default()
        },
    ));
}
fn poll(
    time: Res<Time>,
    files: Res<ExplorerFiles>,
    dirs: Option<Res<NotesDirs>>,
    enabled: Option<Res<MapEnabled>>,
    mut knowledge: ResMut<Knowledge>,
    mut stores: ResMut<NotesStores>,
) {
    let available = map_actor(&files, dirs.as_deref(), enabled.as_deref()).is_some();
    let changed = knowledge.enabled != available || knowledge.folder.as_ref() != Some(&files.dir);
    knowledge.enabled = available;
    knowledge.folder = Some(files.dir.clone());
    let due = knowledge.timer.tick(time.delta()).just_finished();
    if !available {
        if !knowledge.landmarks.is_empty() || changed {
            knowledge.landmarks.clear();
            knowledge.dirty = true;
        }
        return;
    }
    if !changed && !due {
        return;
    }
    match stores.map_for(&files.dir).and_then(|store| store.list()) {
        Ok(landmarks) => {
            if landmarks != knowledge.landmarks || changed {
                knowledge.landmarks = landmarks;
                knowledge.dirty = true;
            }
        }
        Err(error) => warn!("landmark markers: {error}"),
    }
}
fn rebuild(
    mut commands: Commands,
    mut knowledge: ResMut<Knowledge>,
    roots: Query<Entity, With<LandmarkLayer>>,
) {
    if !knowledge.dirty {
        return;
    }
    knowledge.dirty = false;
    let Ok(root) = roots.single() else {
        return;
    };
    commands
        .entity(root)
        .despawn_children()
        .with_children(|root| {
            for landmark in &knowledge.landmarks {
                root.spawn((
                    LandmarkMarker {
                        id: landmark.id,
                        pos: landmark.pos,
                    },
                    Name::new(format!("landmark #{}", landmark.id)),
                    Pickable::IGNORE,
                    Node {
                        position_type: PositionType::Absolute,
                        width: px(1),
                        height: px(1),
                        ..default()
                    },
                ))
                .with_children(|marker| {
                    marker.spawn((
                        Pickable::IGNORE,
                        Node {
                            position_type: PositionType::Absolute,
                            left: px(-6),
                            top: px(-6),
                            width: px(12),
                            height: px(12),
                            border: UiRect::all(px(2)),
                            border_radius: BorderRadius::all(px(6)),
                            ..default()
                        },
                        BackgroundColor(MARK),
                        BorderColor::all(Color::srgb(0.10, 0.10, 0.08)),
                    ));
                    let text = format!("#{} {}", landmark.id, landmark.name);
                    let size = label_size(&text);
                    marker.spawn((
                        LandmarkLabel {
                            id: landmark.id,
                            fallback_size: size,
                        },
                        Pickable::IGNORE,
                        Text::new(text),
                        TextFont {
                            font_size: FontSize::Px(12.0),
                            ..default()
                        },
                        TextColor(MARK),
                        BackgroundColor(Color::srgba(0.03, 0.04, 0.05, 0.92)),
                        Node {
                            position_type: PositionType::Absolute,
                            left: px(9),
                            top: px(-12),
                            width: px(size.x),
                            max_width: px(240),
                            flex_shrink: 0.0,
                            padding: UiRect::axes(px(5), px(3)),
                            border_radius: BorderRadius::all(px(3)),
                            ..default()
                        },
                    ));
                });
            }
        });
}
fn project(
    cameras: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    mut markers: Query<(&LandmarkMarker, &mut Node), Without<LandmarkLabel>>,
    mut labels: Query<(&LandmarkLabel, &ComputedNode, &mut Node), Without<LandmarkMarker>>,
) {
    let Ok((camera, transform)) = cameras.single() else {
        return;
    };
    let viewport = camera.logical_viewport_rect();
    let mut anchors = BTreeMap::new();
    for (marker, mut node) in &mut markers {
        let point = sim_to_world(SimPos::new(marker.pos[0], marker.pos[1])) + Vec3::Y * 0.10;
        match camera.world_to_viewport(transform, point) {
            Ok(position) if viewport.is_none_or(|rect| rect.contains(position)) => {
                // Bevy 0.20 already includes the logical viewport origin.
                node.left = px(position.x);
                node.top = px(position.y);
                node.display = Display::Flex;
                anchors.insert(marker.id, position);
            }
            _ => node.display = Display::None,
        }
    }
    let mut requests = Vec::new();
    for (label, computed, _) in &mut labels {
        if let Some(anchor) = anchors.get(&label.id) {
            // ComputedNode uses physical pixels; placement uses logical px.
            let measured = computed.size() * computed.inverse_scale_factor();
            let size = if measured.x > 0.0 && measured.y > 0.0 {
                measured
            } else {
                label.fallback_size
            };
            requests.push(LabelRequest {
                id: label.id,
                anchor: *anchor,
                size,
            });
        }
    }
    let placements = place_labels(requests, viewport);
    for (label, _, mut node) in &mut labels {
        if let (Some(rect), Some(anchor)) = (placements.get(&label.id), anchors.get(&label.id)) {
            let offset = rect.min - *anchor;
            node.left = px(offset.x);
            node.top = px(offset.y);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use npc_framework::agent_io::map::{LandmarkChange, NewLandmark, Place};
    use npc_framework::agent_io::notes::{Stamp, Writer};
    use npc_framework::prelude::SimId;
    use std::collections::BTreeMap;
    fn app(tag: &str) -> (App, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("landmark-view-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<NotesStores>()
            .insert_resource(ExplorerFiles { dir: dir.clone() })
            .insert_resource(NotesDirs(BTreeMap::from([(SimId(1), dir.clone())])))
            .add_plugins(plugin);
        (app, dir)
    }
    fn add(app: &mut App, dir: &std::path::Path) -> Landmark {
        app.world_mut()
            .resource_mut::<NotesStores>()
            .map_for(dir)
            .unwrap()
            .add(
                NewLandmark {
                    name: "Beyond the wall".into(),
                    place: Place::Pos([21000, -18000]),
                    kind: Some("gate".into()),
                    note: "Known through the map".into(),
                },
                [21000, -18000],
                None,
                Stamp {
                    tick: 1,
                    date: String::new(),
                },
                Writer::Player,
            )
            .unwrap()
    }
    #[test]
    fn coincident_and_nearby_labels_pack_without_moving_their_anchors() {
        let requests = vec![
            LabelRequest {
                id: 3,
                anchor: Vec2::new(102.0, 101.0),
                size: Vec2::new(160.0, 24.0),
            },
            LabelRequest {
                id: 1,
                anchor: Vec2::new(100.0, 100.0),
                size: Vec2::new(120.0, 24.0),
            },
            LabelRequest {
                id: 2,
                anchor: Vec2::new(100.0, 100.0),
                size: Vec2::new(140.0, 24.0),
            },
        ];
        let viewport = Some(Rect::from_corners(Vec2::ZERO, Vec2::new(800.0, 600.0)));
        let placed = place_labels(requests.clone(), viewport);
        let mut reversed = requests.clone();
        reversed.reverse();
        assert_eq!(placed, place_labels(reversed, viewport));
        for (id, rect) in &placed {
            for (other_id, other) in &placed {
                if id != other_id {
                    assert!(
                        rect.intersect(*other).is_empty(),
                        "labels {id} and {other_id} overlap"
                    );
                }
            }
        }
        assert_eq!(requests[1].anchor, Vec2::new(100.0, 100.0));
        assert_eq!(requests[2].anchor, requests[1].anchor);
        assert_ne!(placed[&1].min, placed[&2].min);
    }
    #[test]
    fn short_landmark_labels_have_room_on_one_line_and_long_labels_are_bounded() {
        let short = label_size("#2 Return point");
        assert!(short.x >= 120.0);
        assert_eq!(short.y, 22.0);
        let long = label_size(&"x".repeat(200));
        assert_eq!(long.x, 240.0);
        assert!(long.y > short.y);
    }
    #[test]
    fn markers_follow_the_store_and_never_follow_the_thing() {
        let (mut app, dir) = app("follow");
        let landmark = add(&mut app, &dir);
        app.update();
        let marker = app
            .world_mut()
            .query::<&LandmarkMarker>()
            .single(app.world())
            .unwrap();
        assert_eq!(marker.pos, [21000, -18000]);
        app.world_mut()
            .resource_mut::<NotesStores>()
            .map_for(&dir)
            .unwrap()
            .update(
                landmark.id,
                &landmark.updated,
                LandmarkChange {
                    pos: Some([-5000, 6000]),
                    name: Some("Renamed landmark".into()),
                    ..default()
                },
                Stamp {
                    tick: 2,
                    date: String::new(),
                },
                Writer::Player,
            )
            .unwrap();
        app.world_mut()
            .resource_mut::<Knowledge>()
            .timer
            .set_elapsed(POLL);
        app.update();
        assert_eq!(
            app.world_mut()
                .query::<&LandmarkMarker>()
                .single(app.world())
                .unwrap()
                .pos,
            [-5000, 6000]
        );
        assert!(
            app.world_mut()
                .query::<&Text>()
                .iter(app.world())
                .any(|t| t.0.contains("Renamed landmark"))
        );
        let _ = std::fs::remove_dir_all(dir);
    }
    #[test]
    fn markers_show_under_fog_and_are_never_pickable() {
        let (mut app, dir) = app("fog");
        add(&mut app, &dir);
        app.insert_resource(crate::FogOfWar { enabled: true });
        app.update();
        let marker = app
            .world_mut()
            .query_filtered::<Entity, With<LandmarkMarker>>()
            .single(app.world())
            .unwrap();
        assert_eq!(
            app.world().get::<Node>(marker).unwrap().display,
            Display::Flex
        );
        assert_eq!(app.world().get::<Pickable>(marker), Some(&Pickable::IGNORE));
        assert!(app.world().get::<crate::VisualOf>(marker).is_none());
        app.world_mut().resource_mut::<crate::FogOfWar>().enabled = false;
        app.update();
        assert!(app.world().get::<Node>(marker).is_some());
        app.insert_resource(MapEnabled(false));
        app.update();
        assert_eq!(
            app.world_mut()
                .query::<&LandmarkMarker>()
                .iter(app.world())
                .count(),
            0
        );
        let _ = std::fs::remove_dir_all(dir);
    }
    #[test]
    fn map_off_and_unmapped_folder_do_not_open_a_store() {
        let (mut app, dir) = app("off");
        app.insert_resource(MapEnabled(false));
        app.update();
        assert!(!dir.exists());
        app.insert_resource(MapEnabled(true));
        app.world_mut().resource_mut::<NotesDirs>().0.clear();
        app.update();
        assert!(!dir.exists());
    }
}
