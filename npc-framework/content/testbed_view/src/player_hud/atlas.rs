//! A published settlement chart, with only the controlled actor's live location.
//! No foreign inventory, unseen residents or undiscovered live state enters this map.
//!
//! The chart only shows: the rows under it ("Travel to Harbour") are how a
//! destination is chosen, so it carries no buttons of its own.
use super::*;
use crate::game_ui::tokens::{self as t, Surface};
use crate::game_ui::widgets::{self as w, Tone};
use bevy::picking::Pickable;

#[derive(Component)]
struct AtlasCanvas;
#[derive(Component)]
struct AtlasPlayer;

fn point(pos: SimPos, width: f32, height: f32) -> (f32, f32) {
    (
        (50. + pos.x as f32 / width * 100.).clamp(2., 95.),
        (50. - pos.z as f32 / height * 100.).clamp(2., 94.),
    )
}
pub(super) fn refresh(world: &mut World, observed: &Value) {
    let Some(parent) = world
        .resource::<PlayerState>()
        .rows
        .get("regional_chart")
        .copied()
    else {
        return;
    };
    let chart = &observed["sections"]["living_world"];
    let width = chart["width_mm"].as_u64().unwrap_or(256000).max(1) as f32;
    let height = chart["height_mm"].as_u64().unwrap_or(256000).max(1) as f32;
    if world
        .query_filtered::<Entity, With<AtlasCanvas>>()
        .iter(world)
        .next()
        .is_none()
    {
        // Solid chrome on the paper panel, like a toast: a small dark map.
        let canvas = world
            .spawn((
                ChildOf(parent),
                AtlasCanvas,
                Pickable::IGNORE,
                Node {
                    width: percent(100),
                    height: px(132),
                    min_height: px(132),
                    margin: UiRect::top(px(6)),
                    border: UiRect::all(px(1)),
                    border_radius: BorderRadius::all(px(t::RADIUS_MD)),
                    overflow: Overflow::clip(),
                    ..default()
                },
                BackgroundColor(t::BARK_800),
                BorderColor::all(t::BARK_600),
            ))
            .id();
        let north = w::label(
            world,
            canvas,
            "North",
            t::CAPTION,
            Tone::caption(Surface::Chrome),
        );
        world.entity_mut(north).insert(Node {
            position_type: PositionType::Absolute,
            right: px(8),
            top: px(5),
            ..default()
        });
        for location in model::array(&chart["chart"]) {
            let Some(pos) = model::position(&location["position"]) else {
                continue;
            };
            let (x, y) = point(pos, width, height);
            let mark = world
                .spawn((
                    ChildOf(canvas),
                    Node {
                        position_type: PositionType::Absolute,
                        left: percent(x - 2.),
                        top: percent(y),
                        flex_direction: FlexDirection::Column,
                        row_gap: px(3),
                        ..default()
                    },
                ))
                .id();
            // A settlement is a honey square; you are a light ring.
            world.spawn((
                ChildOf(mark),
                Node {
                    width: px(8),
                    height: px(8),
                    ..default()
                },
                BackgroundColor(t::HONEY_500),
            ));
            w::label(
                world,
                mark,
                location["name"].as_str().unwrap_or("Settlement"),
                t::CAPTION,
                Tone::text(Surface::Chrome),
            );
        }
        world.spawn((
            ChildOf(canvas),
            AtlasPlayer,
            Node {
                position_type: PositionType::Absolute,
                width: px(10),
                height: px(10),
                border: UiRect::all(px(2)),
                border_radius: BorderRadius::all(px(5)),
                ..default()
            },
            BorderColor::all(t::CREAM_50),
        ));
        let key = w::label(
            world,
            canvas,
            "Ring: you \u{b7} squares: settlements",
            t::CAPTION,
            Tone::caption(Surface::Chrome),
        );
        world.entity_mut(key).insert(Node {
            position_type: PositionType::Absolute,
            left: px(8),
            bottom: px(5),
            ..default()
        });
    }
    if let Some(pos) = model::position(&observed["sections"]["self"]["pos"]) {
        let (x, y) = point(pos, width, height);
        for mut node in world
            .query_filtered::<&mut Node, With<AtlasPlayer>>()
            .iter_mut(world)
        {
            node.left = percent(x);
            node.top = percent(y);
        }
    }
}
