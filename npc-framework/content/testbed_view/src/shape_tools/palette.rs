//! Shape mode's interface: the tool rail, the brush bar and the tag that
//! follows the cursor. Widgets only: they queue [`Action`]s for the tool
//! state and never touch terrain themselves.
use super::model::{self, Falloff, Tool};
use super::{Action, SURFACES, ShapeTools, picking, surface_name};
use crate::IslandMode;
use crate::game_ui::icon::{self, Icon};
use crate::game_ui::scale;
use crate::game_ui::tokens::{self as t, Status, Surface};
use crate::game_ui::tooltip::Tooltip;
use crate::game_ui::widgets::{self as w, Kind, Reach, SliderParts, Tone};
use crate::island_shell::focus::FocusRegion;
use bevy::picking::Pickable;
use bevy::prelude::*;
use bevy::ui::InteractionDisabled;
use bevy::ui_widgets::{Activate, SliderRange, SliderValue, ValueChange};
use npc_framework::prelude::SimControl;
use npc_framework::view_helpers::ViewSystems;
use testbed_content::village::terrain::TerrainSurface;

/// Either of Shape's two panels: the rail and the brush bar.
#[derive(Component)]
pub(crate) struct ShapeRoot;
#[derive(Component)]
struct RailRoot;
#[derive(Component)]
struct BrushRoot;
#[derive(Component, Clone, Copy)]
struct ShapeAction(Action);
/// Holds the kit toast for the tool's status line.
#[derive(Component)]
struct StatusLane;
#[derive(Component)]
struct CursorTag;

/// What the panels spawned, and what the rebuilt parts last showed.
#[derive(Resource, Default)]
struct Parts {
    size: Option<SliderParts>,
    history: Option<Entity>,
    /// The colour squares of the Surface group: dimmed unless Paint is the
    /// tool.
    squares: Vec<(Entity, [f32; 3], TerrainSurface)>,
    status: Option<(model::Weight, String)>,
    /// Seconds the status toast still stays.
    status_left: f32,
    /// The notice shown is "edits wait for the world to run".
    waited: bool,
    tag: Option<(model::Tag, Tool)>,
    tag_parts: Option<TagParts>,
}

/// Width of the cursor tag while it carries a refusal.
const REFUSAL_WIDTH: f32 = 280.0;

/// The cursor tag's own nodes: filled in place, so a height that changes
/// under a moving brush never rebuilds text.
#[derive(Clone, Copy)]
struct TagParts {
    /// Holds the tool's icon.
    picture: Entity,
    line: Entity,
    /// The refusal's row: a cross and the reason.
    refusal: Entity,
    reason: Entity,
}

pub(super) fn plugin(app: &mut App) {
    crate::game_ui::install(app);
    app.init_resource::<Parts>()
        .add_observer(activate)
        .add_systems(Startup, spawn)
        .add_systems(
            Update,
            refresh.in_set(ViewSystems::Present).after(super::process),
        );
}

fn tool_icon(tool: Tool) -> Icon {
    match tool {
        Tool::Raise => Icon::Raise,
        Tool::Lower => Icon::Lower,
        Tool::Flatten => Icon::Flatten,
        Tool::Smooth => Icon::Smooth,
        Tool::Paint => Icon::Paint,
    }
}

fn caption(world: &mut World, parent: Entity, text: &str) -> Entity {
    w::label(
        world,
        parent,
        text,
        t::CAPTION,
        Tone::secondary(Surface::Chrome),
    )
}

fn group(world: &mut World, parent: Entity) -> Entity {
    world
        .spawn((
            ChildOf(parent),
            Node {
                align_items: AlignItems::Center,
                column_gap: px(6),
                flex_shrink: 0.0,
                ..default()
            },
        ))
        .id()
}

/// The rail and the brush bar for a test that walks the Tab ring.
#[cfg(test)]
pub(crate) fn spawn_for_tests(world: &mut World) {
    world.init_resource::<Parts>();
    spawn(world);
}

fn spawn(world: &mut World) {
    let mut parts = Parts::default();

    // The tool rail, left, under the bar.
    let rail = world
        .spawn((
            Name::new("Shape tools"),
            ShapeRoot,
            RailRoot,
            FocusRegion::Palette,
            GlobalZIndex(t::layer::CHROME),
            w::chrome(
                Node {
                    position_type: PositionType::Absolute,
                    top: px(t::TOP_BAR + t::SCREEN_MARGIN),
                    left: px(t::SCREEN_MARGIN),
                    width: px(76),
                    padding: UiRect::axes(px(5), px(6)),
                    row_gap: px(4),
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    display: Display::None,
                    ..default()
                },
                t::RADIUS_LG,
            ),
        ))
        .id();
    for tool in Tool::ALL {
        let tile = w::dock_tile(
            world,
            rail,
            tool.name(),
            tool_icon(tool),
            Some(model::tool_key(tool)),
            ShapeAction(Action::Tool(tool)),
        );
        world.entity_mut(tile.entity).insert(
            Tooltip::new(tool.name(), tool.help())
                .icon(tool_icon(tool))
                .key(model::tool_key(tool)),
        );
    }
    let rule = w::divider(world, rail, Surface::Chrome, false);
    if let Some(mut node) = world.get_mut::<Node>(rule) {
        node.width = percent(100);
        node.margin = UiRect::vertical(px(2));
    }
    // Resources: place and remove patches (the regional world only).
    if testbed_content::village::frontier::enabled(world) {
        super::resources::rail_tiles(world, rail);
        super::placing::rail_tile(world, rail);
        let rule = w::divider(world, rail, Surface::Chrome, false);
        if let Some(mut node) = world.get_mut::<Node>(rule) {
            node.width = percent(100);
            node.margin = UiRect::vertical(px(2));
        }
    }
    for (name, picture, action, key, sentence) in [
        (
            "Undo",
            Icon::Undo,
            Action::Undo,
            "Ctrl+Z",
            "Take back the last stroke, plot or prop.",
        ),
        (
            "Redo",
            Icon::Redo,
            Action::Redo,
            "Ctrl+Y",
            "Lay the stroke, plot or prop you took back again.",
        ),
    ] {
        let tile = w::dock_tile(world, rail, name, picture, None, ShapeAction(action));
        world.entity_mut(tile.entity).insert(
            Tooltip::new(name, sentence)
                .icon(picture)
                .key(key)
                .detail([model::HISTORY_SENTENCE]),
        );
    }
    // How many strokes can be taken back, of how many are kept.
    parts.history = Some(caption(world, rail, ""));
    // The footer: write the shaped world to a world file.
    if testbed_content::village::frontier::enabled(world) {
        let rule = w::divider(world, rail, Surface::Chrome, false);
        if let Some(mut node) = world.get_mut::<Node>(rule) {
            node.width = percent(100);
            node.margin = UiRect::vertical(px(2));
        }
        super::resources::export_tile(world, rail);
    }

    // The brush bar, bottom centre.
    let bar = world
        .spawn((
            Name::new("Shape brush"),
            ShapeRoot,
            BrushRoot,
            FocusRegion::Palette,
            GlobalZIndex(t::layer::CHROME),
            w::chrome(
                Node {
                    position_type: PositionType::Absolute,
                    left: px(160),
                    bottom: px(t::SCREEN_MARGIN),
                    height: px(t::DOCK),
                    padding: UiRect::horizontal(px(12)),
                    column_gap: px(12),
                    align_items: AlignItems::Center,
                    display: Display::None,
                    ..default()
                },
                t::RADIUS_LG,
            ),
        ))
        .id();
    // Size: the slider, its two keys and the value in metres.
    let size = group(world, bar);
    caption(world, size, "Size");
    w::keycap(world, size, Surface::Chrome, "[");
    let slider = w::slider(
        world,
        size,
        "Brush size",
        Surface::Chrome,
        (0.0, 8.0),
        1.0,
        0.0,
        "0 m",
        Reach::Ring,
        (),
    );
    world.entity_mut(slider.entity).observe(resize).insert(
        Tooltip::new(
            "Brush size",
            "How far the brush reaches from the cell under the cursor.",
        )
        .key("[ ]")
        .detail(["The world sets the largest size."]),
    );
    // The keys sit around the track; the value follows the upper one.
    w::keycap(world, size, Surface::Chrome, "]");
    world.entity_mut(slider.value).insert(ChildOf(size));
    parts.size = Some(slider);
    w::divider(world, bar, Surface::Chrome, true);
    // Edge: hard or soft.
    let edge = group(world, bar);
    caption(world, edge, "Edge");
    let edges = w::segmented(world, edge, Surface::Chrome);
    for falloff in [Falloff::Hard, Falloff::Soft] {
        let name = model::falloff_name(falloff);
        let segment = w::segment(
            world,
            edges,
            name,
            None,
            Some(name),
            None,
            Reach::Ring,
            ShapeAction(Action::Edge(falloff)),
        );
        world
            .entity_mut(segment)
            .insert(Tooltip::new(format!("{name} edge"), model::falloff_help(falloff)).key("H"));
    }
    w::keycap(world, edge, Surface::Chrome, "H");
    w::divider(world, bar, Surface::Chrome, true);
    // Strength: how far one Raise or Lower dab moves the ground.
    let strength = group(world, bar);
    caption(world, strength, "Strength");
    let strengths = w::segmented(world, strength, Surface::Chrome);
    for (name, step_mm) in model::STRENGTHS {
        let segment = w::segment(
            world,
            strengths,
            name,
            None,
            Some(name),
            None,
            Reach::Ring,
            ShapeAction(Action::Strength(step_mm)),
        );
        world.entity_mut(segment).insert(Tooltip::new(
            format!("{name} strength"),
            model::strength_help(step_mm),
        ));
    }
    w::divider(world, bar, Surface::Chrome, true);
    // Surface: what Paint lays.
    let surface = group(world, bar);
    caption(world, surface, "Surface");
    for kind in SURFACES {
        let swatch = w::control(
            world,
            surface,
            surface_name(kind),
            Kind::Quiet,
            Surface::Chrome,
            Reach::Ring,
            Node {
                width: px(32),
                height: px(32),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                border: UiRect::all(px(1)),
                border_radius: BorderRadius::all(px(t::RADIUS_MD)),
                flex_shrink: 0.0,
                ..default()
            },
            ShapeAction(Action::Surface(kind)),
        );
        let [r, g, b] = model::surface_swatch(kind);
        let square = world
            .spawn((
                ChildOf(swatch),
                Pickable::IGNORE,
                Node {
                    width: px(22),
                    height: px(22),
                    border: UiRect::all(px(2)),
                    border_radius: BorderRadius::all(px(t::RADIUS_SM)),
                    ..default()
                },
                BackgroundColor(Color::srgb(r, g, b)),
                BorderColor::all(t::BARK_600),
            ))
            .id();
        parts.squares.push((square, [r, g, b], kind));
        world.entity_mut(swatch).insert(
            Tooltip::new(
                surface_name(kind),
                "Choose this surface; Paint lays it and keeps the heights.",
            )
            .key("V")
            .detail(["V steps to the next surface and takes up Paint."]),
        );
    }
    w::keycap(world, surface, Surface::Chrome, "V");

    // The tool's own word on what just happened, above the brush bar.
    world.spawn((
        Name::new("Shape status"),
        StatusLane,
        Pickable::IGNORE,
        GlobalZIndex(t::layer::TOAST),
        Node {
            position_type: PositionType::Absolute,
            left: px(0),
            right: px(0),
            bottom: px(t::SCREEN_MARGIN + t::DOCK + 12.0),
            justify_content: JustifyContent::Center,
            display: Display::None,
            ..default()
        },
    ));

    // The tag beside the cursor.
    let tag = world
        .spawn((
            Name::new("Shape cursor tag"),
            CursorTag,
            Pickable::IGNORE,
            GlobalZIndex(t::layer::TOOLTIP),
            // The critical edge of a refusal; the kit keeps the border.
            Outline::new(px(1), px(0), Color::NONE),
            w::chrome(
                Node {
                    position_type: PositionType::Absolute,
                    left: px(0),
                    top: px(0),
                    padding: UiRect::axes(px(10), px(6)),
                    flex_direction: FlexDirection::Column,
                    row_gap: px(3),
                    display: Display::None,
                    ..default()
                },
                t::RADIUS_MD,
            ),
        ))
        .id();
    let row = |world: &mut World| {
        world
            .spawn((
                ChildOf(tag),
                Node {
                    align_items: AlignItems::Center,
                    column_gap: px(6),
                    ..default()
                },
            ))
            .id()
    };
    let head = row(world);
    let picture = world
        .spawn((
            ChildOf(head),
            Node {
                width: px(14),
                height: px(14),
                flex_shrink: 0.0,
                ..default()
            },
        ))
        .id();
    let line = w::label(world, head, "", t::BODY, Tone::text(Surface::Chrome));
    // A refusal: a cross and the simulation's reason, never colour alone.
    let refusal = row(world);
    icon::icon(
        world,
        refusal,
        Icon::Close,
        12.0,
        Tone::status(Surface::Chrome, Status::Crit),
    );
    let reason = w::paragraph(world, refusal, "", t::DENSE, Tone::text(Surface::Chrome));
    // Definite widths: a refusal makes the tag REFUSAL_WIDTH wide and its
    // sentence wraps inside that, so the tag grows downwards with it.
    if let Some(mut node) = world.get_mut::<Node>(reason) {
        node.width = px(REFUSAL_WIDTH - 22.0 - 18.0);
        node.flex_shrink = 0.0;
    }
    if let Some(mut node) = world.get_mut::<Node>(refusal) {
        node.align_items = AlignItems::FlexStart;
    }
    w::show(world, refusal, false);
    ignore_pointer(world, tag);
    parts.tag_parts = Some(TagParts {
        picture,
        line,
        refusal,
        reason,
    });
    *world.resource_mut::<Parts>() = parts;
}

fn activate(
    event: On<Activate>,
    actions: Query<(&ShapeAction, Has<InteractionDisabled>)>,
    mut state: ResMut<ShapeTools>,
) {
    if let Ok((action, false)) = actions.get(event.entity) {
        state.requests.push(action.0);
    }
}

/// The slider moved: the brush takes its whole metres.
fn resize(change: On<ValueChange<f32>>, mut state: ResMut<ShapeTools>) {
    state
        .requests
        .push(Action::SetRadius(change.value.round().max(0.0) as u32));
}

fn ignore_pointer(world: &mut World, entity: Entity) {
    world.entity_mut(entity).insert(Pickable::IGNORE);
    let children = world
        .get::<Children>(entity)
        .map(|children| children.to_vec())
        .unwrap_or_default();
    for child in children {
        ignore_pointer(world, child);
    }
}

/// Fill the tag in place. Only the tool's icon is built again, and only
/// when the tool changes.
fn fill_tag(
    world: &mut World,
    root: Entity,
    parts: TagParts,
    tag: &model::Tag,
    tool: Tool,
    tool_changed: bool,
) {
    if tool_changed {
        world.entity_mut(parts.picture).despawn_children();
        let picture = icon::icon(
            world,
            parts.picture,
            tool_icon(tool),
            14.0,
            Tone::text(Surface::Chrome),
        );
        world.entity_mut(picture).insert(Pickable::IGNORE);
    }
    w::set_text(world, parts.line, &tag.line);
    if let Some(reason) = &tag.reason {
        w::set_text(world, parts.reason, reason);
    }
    w::show(world, parts.refusal, tag.reason.is_some());
    let refused = tag.reason.is_some();
    let width = if refused {
        px(REFUSAL_WIDTH)
    } else {
        Val::Auto
    };
    if let Some(mut node) = world.get_mut::<Node>(root)
        && node.width != width
    {
        node.width = width;
    }
    let edge = if refused { t::CRIT_LIGHT } else { Color::NONE };
    if let Some(mut outline) = world.get_mut::<Outline>(root)
        && outline.color != edge
    {
        outline.color = edge;
    }
}

fn refresh(world: &mut World) {
    let shown = world.get_resource::<IslandMode>() == Some(&IslandMode::Editor)
        && !crate::player_hud::active(world);
    let top = crate::island_shell::content_top(world);
    let layout = scale::layout(world);
    for (mut node, rail, computed) in world
        .query_filtered::<(&mut Node, Has<RailRoot>, &ComputedNode), With<ShapeRoot>>()
        .iter_mut(world)
    {
        let display = if shown { Display::Flex } else { Display::None };
        if node.display != display {
            node.display = display;
        }
        if rail {
            if node.top != px(top) {
                node.top = px(top);
            }
        } else {
            // Centred, whatever its measured width.
            let width = computed.size().x * computed.inverse_scale_factor();
            let left = px(((layout.width - width) * 0.5).round().max(t::SCREEN_MARGIN));
            if width > 0.0 && node.left != left {
                node.left = left;
            }
        }
    }
    if !shown {
        for mut node in world
            .query_filtered::<&mut Node, Or<(With<CursorTag>, With<StatusLane>)>>()
            .iter_mut(world)
        {
            node.display = Display::None;
        }
        return;
    }
    let limits = picking::limits(world);
    let state = world.resource::<ShapeTools>();
    let brush = state.brush;
    let radius = brush.radius.min(limits.max_radius);
    let (can_undo, can_redo, queued) = (state.can_undo(), state.can_redo(), state.queued());
    let (depth, kept) = state.history_depth();
    let hover = state.hover;
    let reason = state.cursor_reason().map(str::to_owned);
    let stopped = world
        .get_resource::<SimControl>()
        .is_some_and(SimControl::is_stopped);
    let status = model::status_text(&state.status, queued, stopped);
    let resource_tool = super::other_tool(world);
    let can_undo = can_undo || super::placing::can_step(world, true);
    let can_redo = can_redo || super::placing::can_step(world, false);
    let buttons: Vec<_> = world
        .query::<(Entity, &ShapeAction)>()
        .iter(world)
        .map(|(entity, action)| (entity, action.0))
        .collect();
    for (entity, action) in buttons {
        let (on, enabled) = match action {
            Action::Tool(tool) => (tool == brush.tool && !resource_tool, true),
            Action::Surface(surface) => {
                (surface == brush.surface && brush.tool == Tool::Paint, true)
            }
            Action::Edge(falloff) => (falloff == brush.falloff, true),
            Action::Strength(step_mm) => (step_mm == brush.step_mm, true),
            // Dimmed when there is nothing to take back or lay again.
            Action::Undo => (false, can_undo),
            Action::Redo => (false, can_redo),
            Action::Radius(_) | Action::SetRadius(_) | Action::Falloff | Action::NextSurface => {
                (false, true)
            }
        };
        w::set_state(world, entity, on, false);
        w::set_enabled(world, entity, enabled);
    }
    let parts = world.resource::<Parts>();
    let (size, history, squares) = (parts.size, parts.history, parts.squares.clone());
    if let Some(size) = size {
        let most = limits.max_radius as f32;
        if world
            .get::<SliderRange>(size.entity)
            .is_none_or(|range| range.end() != most)
        {
            world
                .entity_mut(size.entity)
                .insert(SliderRange::new(0.0, most));
        }
        if world.get::<SliderValue>(size.entity).map(|value| value.0) != Some(radius as f32) {
            world
                .entity_mut(size.entity)
                .insert(SliderValue(radius as f32));
        }
        w::set_text(world, size.value, &model::size_text(radius));
    }
    if let Some(history) = history {
        w::set_text(world, history, &model::history_text(depth, kept));
    }
    // The surfaces belong to Paint: dimmed, not hidden, under another tool.
    let alpha = if brush.tool == Tool::Paint { 1.0 } else { 0.40 };
    for (square, [r, g, b], kind) in squares {
        let wanted = Color::srgba(r, g, b, alpha);
        if let Some(mut color) = world.get_mut::<BackgroundColor>(square)
            && color.0 != wanted
        {
            color.0 = wanted;
        }
        // The surface Paint lays has a light ring; the others a dark edge.
        let ring = BorderColor::all(if kind == brush.surface && brush.tool == Tool::Paint {
            t::CREAM_50
        } else {
            t::BARK_600
        });
        if let Some(mut border) = world.get_mut::<BorderColor>(square)
            && *border != ring
        {
            *border = ring;
        }
    }
    // The tool's word, as a toast above the brush bar. It leaves by itself,
    // except while edits wait for a stopped world.
    let delta = world.get_resource::<Time>().map_or(0.0, Time::delta_secs);
    let waiting = queued > 0 && stopped;
    let changed = world.resource::<Parts>().status != status;
    let lanes: Vec<Entity> = world
        .query_filtered::<Entity, With<StatusLane>>()
        .iter(world)
        .collect();
    if changed {
        // A notice stays its time even when the tool's word moves on to a
        // success, which says nothing; only the next notice replaces it.
        // "Edits wait" leaves as soon as nothing waits.
        if let Some((weight, text)) = &status {
            let (tone, picture) = match weight {
                model::Weight::Plain => (Status::Info, Icon::Hint),
                model::Weight::Warning => (Status::Warn, Icon::status(Status::Warn)),
                model::Weight::Refusal => (Status::Crit, Icon::status(Status::Crit)),
            };
            for lane in &lanes {
                world.entity_mut(*lane).despawn_children();
                let toast = w::toast(world, *lane, tone, text, Some(picture));
                ignore_pointer(world, toast);
            }
        }
        let mut parts = world.resource_mut::<Parts>();
        match &status {
            Some((model::Weight::Refusal, _)) => parts.status_left = t::motion::TOAST_STAY_ALERT,
            Some(_) => parts.status_left = t::motion::TOAST_STAY,
            None if parts.waited => parts.status_left = 0.0,
            None => {}
        }
        parts.waited = waiting;
        parts.status = status;
    }
    let left = {
        let mut parts = world.resource_mut::<Parts>();
        if !waiting {
            parts.status_left = (parts.status_left - delta).max(0.0);
        }
        parts.status_left
    };
    for lane in lanes {
        if let Some(mut node) = world.get_mut::<Node>(lane) {
            let display = if left > 0.0 || waiting {
                Display::Flex
            } else {
                Display::None
            };
            if node.display != display {
                node.display = display;
            }
        }
    }

    // The tag beside the cursor: the tool, the ground under it, any refusal.
    let cursor = world
        .query::<&Window>()
        .iter(world)
        .find_map(Window::cursor_position);
    let here = hover.and_then(|(x, z)| picking::terrain_cell(world, x, z));
    let tag = here.map(|cell| (model::tag(brush.tool, cell, reason.as_deref()), brush.tool));
    let roots: Vec<Entity> = world
        .query_filtered::<Entity, With<CursorTag>>()
        .iter(world)
        .collect();
    if world.resource::<Parts>().tag != tag {
        let parts = world.resource::<Parts>();
        let shown_tool = parts.tag.as_ref().map(|(_, tool)| *tool);
        if let (Some((tag, tool)), Some(tag_parts)) = (&tag, parts.tag_parts) {
            for root in &roots {
                fill_tag(
                    world,
                    *root,
                    tag_parts,
                    tag,
                    *tool,
                    shown_tool != Some(*tool),
                );
            }
        }
        // The tool last drawn stays known while the cursor is off the ground.
        if tag.is_some() {
            world.resource_mut::<Parts>().tag = tag.clone();
        }
    }
    for root in roots {
        let Some(mut node) = world.get_mut::<Node>(root) else {
            continue;
        };
        match (cursor, tag.is_some()) {
            (Some(cursor), true) => {
                node.display = Display::Flex;
                node.left = px(cursor.x / layout.scale.max(0.1) + 18.0);
                node.top = px(cursor.y / layout.scale.max(0.1) + 18.0);
            }
            _ => node.display = Display::None,
        }
    }
}
