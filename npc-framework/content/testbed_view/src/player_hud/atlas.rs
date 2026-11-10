//! A published settlement chart, with only the controlled actor's live location.
//! No foreign inventory, unseen residents or undiscovered live state enters this map.
use super::*;
#[derive(Component)]
struct AtlasCanvas;
#[derive(Component)]
struct AtlasPlayer;
#[derive(Component)]
struct AtlasDestination;

fn point(pos: SimPos, width: f32, height: f32) -> (f32, f32) {
    (
        (50. + pos.x as f32 / width * 100.).clamp(2., 95.),
        (50. - pos.z as f32 / height * 100.).clamp(2., 94.),
    )
}
pub(super) fn refresh(world: &mut World, observed: &Value, ready: bool) {
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
        let canvas = world
            .spawn((
                ChildOf(parent),
                AtlasCanvas,
                Node {
                    width: percent(100),
                    height: px(132),
                    min_height: px(132),
                    margin: UiRect::top(px(6)),
                    overflow: Overflow::clip(),
                    ..default()
                },
                BackgroundColor(Color::srgb(0.085, 0.15, 0.17)),
            ))
            .id();
        world.spawn((
            ChildOf(canvas),
            Node {
                position_type: PositionType::Absolute,
                right: px(8),
                top: px(5),
                ..default()
            },
            ui::text("NORTH", 10., ui::MUTED),
        ));
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
            world.spawn((
                ChildOf(mark),
                Node {
                    width: px(8),
                    height: px(8),
                    ..default()
                },
                BackgroundColor(ui::GOLD),
            ));
            world.spawn((
                ChildOf(mark),
                ui::text(
                    location["name"].as_str().unwrap_or("Settlement"),
                    12.,
                    ui::TEXT,
                ),
            ));
        }
        world.spawn((
            ChildOf(canvas),
            AtlasPlayer,
            Node {
                position_type: PositionType::Absolute,
                width: px(7),
                height: px(7),
                ..default()
            },
            BackgroundColor(Color::srgb(0.64, 0.95, 0.83)),
        ));
        world.spawn((
            ChildOf(canvas),
            Node {
                position_type: PositionType::Absolute,
                left: px(8),
                bottom: px(5),
                ..default()
            },
            ui::text("Mint marker: you · Gold: settlements", 11., ui::MUTED),
        ));
        let destinations = row(world, parent);
        for location in model::array(&chart["chart"]) {
            let Some(pos) = model::position(&location["position"]) else {
                continue;
            };
            let name = location["name"].as_str().unwrap_or("Settlement");
            let destination = button(world, destinations, name, Action::Walk(pos), false);
            world.entity_mut(destination).insert((
                AtlasDestination,
                AccessibleLabel::new(format!("Travel to {name}")),
            ));
            if let Some(mut node) = world.get_mut::<Node>(destination) {
                node.flex_grow = 1.;
                node.flex_basis = percent(30);
                node.min_width = px(90);
                node.padding = UiRect::axes(px(6), px(8));
            }
        }
    }
    let destinations: Vec<_> = world
        .query_filtered::<Entity, With<AtlasDestination>>()
        .iter(world)
        .collect();
    for destination in destinations {
        disabled(world, destination, !ready);
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
