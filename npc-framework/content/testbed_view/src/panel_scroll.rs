//! View-only bounded panel scrolling: visible rail, page controls and overflow hint.
//! Callers keep their own focus/keyboard scope by marking the returned buttons.
//!
//! [`attach_rail`] is the redesigned form: a six-pixel rail that appears only
//! when the content overflows, the wheel, and the panel's own Page keys. It has
//! no paging buttons and no hint text; screens move to it as they adopt the kit.
use bevy::input_focus::InputFocus;
use bevy::input_focus::tab_navigation::TabIndex;
use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use bevy::ui::{InteractionDisabled, UiScale, UiSystems};
use bevy::ui_widgets::{
    Activate, Button as WidgetButton, ControlOrientation, ScrollAreaPlugin, Scrollbar,
    ScrollbarDragState, ScrollbarPlugin, ScrollbarThumb,
};

const TEXT: Color = Color::srgb(0.85, 0.90, 0.85);
const GOLD: Color = Color::srgb(0.95, 0.80, 0.45);
const TRACK: Color = Color::srgb(0.13, 0.16, 0.19);
const BUTTON: Color = Color::srgb(0.20, 0.24, 0.30);

#[derive(Component)]
struct PageScroll {
    target: Entity,
    direction: i32,
}
#[derive(Component)]
struct ScrollHint(Entity);
#[derive(Component)]
struct PanelThumb;
#[derive(Component)]
struct ScrollTypography;
#[derive(Component)]
struct OverflowChrome {
    rail: Entity,
    footer: Entity,
    controls: [Entity; 2],
}
/// A body whose rail shows only while it overflows.
#[derive(Component)]
struct RailOnly {
    rail: Entity,
}
/// A body whose rail lies under it and shows only while it overflows sideways.
#[derive(Component)]
struct RailOnlyX {
    rail: Entity,
}
#[derive(Component)]
struct RailThumb;
#[derive(Resource)]
struct ScrollFont(Handle<Font>);
struct PanelScrollPlugin;

pub(crate) fn install(app: &mut App) {
    if !app.is_plugin_added::<PanelScrollPlugin>() {
        app.add_plugins(PanelScrollPlugin);
    }
}
impl Plugin for PanelScrollPlugin {
    fn build(&self, app: &mut App) {
        // These are also present in rendered hosts. MinimalPlugins fixtures need
        // the scale resource because the stock scrollbar observers require it.
        app.init_resource::<UiScale>().init_resource::<InputFocus>();
        if !app.is_plugin_added::<bevy::ui_widgets::ButtonPlugin>() {
            app.add_plugins(bevy::ui_widgets::ButtonPlugin);
        }
        if !app.is_plugin_added::<ScrollAreaPlugin>() {
            app.add_plugins(ScrollAreaPlugin);
        }
        if !app.is_plugin_added::<ScrollbarPlugin>() {
            app.add_plugins(ScrollbarPlugin);
        }
        app.add_observer(page_activate)
            .add_systems(Startup, load_font)
            .add_systems(PostUpdate, refresh.after(UiSystems::Layout));
    }
}

/// Reparent `body` into a horizontal, shrinkable viewport frame and add a
/// scrollbar plus persistent paging controls. Attach before adding the panel's
/// status/footer, or rely on the preserved original body position.
pub(crate) fn attach(
    world: &mut World,
    root: Entity,
    body: Entity,
    prefix: &str,
    labels: [&str; 2],
) -> [Entity; 2] {
    attach_with_visibility(world, root, body, prefix, labels, false)
}

/// Use the same scrolling mechanics, showing navigation only while the measured
/// body overflows. Small game menus can remain quiet without losing overflow UX.
pub(crate) fn attach_when_needed(
    world: &mut World,
    root: Entity,
    body: Entity,
    prefix: &str,
    labels: [&str; 2],
) -> [Entity; 2] {
    attach_with_visibility(world, root, body, prefix, labels, true)
}

fn attach_with_visibility(
    world: &mut World,
    root: Entity,
    body: Entity,
    prefix: &str,
    labels: [&str; 2],
    overflow_only: bool,
) -> [Entity; 2] {
    let old_position = world
        .get::<Children>(root)
        .and_then(|children| children.iter().position(|child| child == body));
    let frame = world
        .spawn((
            Name::new(format!("{prefix} scroll viewport")),
            ChildOf(root),
            Node {
                flex_direction: FlexDirection::Row,
                column_gap: px(7),
                flex_grow: 1.0,
                flex_shrink: 1.0,
                min_height: px(0),
                min_width: px(0),
                align_items: AlignItems::Stretch,
                ..default()
            },
        ))
        .id();
    world.entity_mut(body).insert(ChildOf(frame));
    if let Some(mut node) = world.get_mut::<Node>(body) {
        node.flex_grow = 1.0;
        node.flex_shrink = 1.0;
        node.min_width = px(0);
        node.width = px(0);
        node.min_height = px(0);
    }
    if let Some(position) = old_position {
        let mut order = world
            .get::<Children>(root)
            .map(|children| children.to_vec())
            .unwrap_or_default();
        order.retain(|entity| *entity != frame);
        order.insert(position.min(order.len()), frame);
        world.entity_mut(root).replace_children(&order);
    }
    let rail = world
        .spawn((
            Name::new(format!("{prefix} scrollbar")),
            ChildOf(frame),
            Hovered::default(),
            Node {
                width: px(10),
                min_width: px(10),
                flex_shrink: 0.0,
                ..default()
            },
            BackgroundColor(TRACK),
            Scrollbar::new(body, ControlOrientation::Vertical, 28.0),
        ))
        .id();
    world.spawn((
        ChildOf(rail),
        PanelThumb,
        Hovered::default(),
        ScrollbarThumb::default(),
        BackgroundColor(Color::srgb(0.71, 0.58, 0.31)),
        BorderColor::all(GOLD),
    ));
    let footer = world
        .spawn((
            ChildOf(root),
            Name::new(format!("{prefix} scroll controls")),
            Node {
                align_items: AlignItems::Center,
                column_gap: px(6),
                flex_shrink: 0.0,
                ..default()
            },
        ))
        .id();
    let above = page_button(world, footer, body, -1, prefix, labels[0]);
    world.spawn((
        ChildOf(footer),
        ScrollHint(body),
        ScrollTypography,
        Text::new("Scroll for more"),
        TextFont {
            font_size: FontSize::Px(11.0),
            ..default()
        },
        TextColor(TEXT),
        TextLayout::default().with_linebreak(bevy::text::LineBreak::WordOrCharacter),
        Node {
            flex_grow: 1.0,
            min_width: px(0),
            ..default()
        },
    ));
    let below = page_button(world, footer, body, 1, prefix, labels[1]);
    if overflow_only {
        world.entity_mut(body).insert(OverflowChrome {
            rail,
            footer,
            controls: [above, below],
        });
        for entity in [rail, footer] {
            world
                .get_mut::<Node>(entity)
                .expect("scroll chrome")
                .display = Display::None;
        }
        for entity in [above, below] {
            world.entity_mut(entity).insert(TabIndex(-1));
        }
    }
    [above, below]
}
/// Put `body` beside a six-pixel rail in the kit's colours. The rail is
/// hidden until the measured body overflows. Returns the rail.
pub(crate) fn attach_rail(world: &mut World, root: Entity, body: Entity, prefix: &str) -> Entity {
    use crate::game_ui::tokens::Surface;
    use crate::game_ui::widgets::{Role, Swatch, Tone};
    let old_position = world
        .get::<Children>(root)
        .and_then(|children| children.iter().position(|child| child == body));
    let frame = world
        .spawn((
            Name::new(format!("{prefix} scroll viewport")),
            ChildOf(root),
            Node {
                flex_direction: FlexDirection::Row,
                column_gap: px(4),
                flex_grow: 1.0,
                flex_shrink: 1.0,
                min_height: px(0),
                min_width: px(0),
                align_items: AlignItems::Stretch,
                ..default()
            },
        ))
        .id();
    world.entity_mut(body).insert(ChildOf(frame));
    if let Some(mut node) = world.get_mut::<Node>(body) {
        node.flex_grow = 1.0;
        node.flex_shrink = 1.0;
        node.min_width = px(0);
        node.width = px(0);
        node.min_height = px(0);
    }
    if let Some(position) = old_position {
        let mut order = world
            .get::<Children>(root)
            .map(|children| children.to_vec())
            .unwrap_or_default();
        order.retain(|entity| *entity != frame);
        order.insert(position.min(order.len()), frame);
        world.entity_mut(root).replace_children(&order);
    }
    let rail = world
        .spawn((
            Name::new(format!("{prefix} scroll rail")),
            ChildOf(frame),
            Hovered::default(),
            Swatch::Fill,
            Tone::new(Surface::Paper, Role::Sunken),
            Node {
                width: px(6),
                min_width: px(6),
                flex_shrink: 0.0,
                border_radius: BorderRadius::all(px(3)),
                display: Display::None,
                ..default()
            },
            BackgroundColor(Color::NONE),
            Scrollbar::new(body, ControlOrientation::Vertical, 28.0),
        ))
        .id();
    world.spawn((
        ChildOf(rail),
        RailThumb,
        Hovered::default(),
        ScrollbarThumb {
            border_radius: BorderRadius::all(px(3)),
            ..default()
        },
        Swatch::Fill,
        Tone::new(Surface::Paper, Role::Control),
        BackgroundColor(Color::NONE),
    ));
    world.entity_mut(body).insert(RailOnly { rail });
    rail
}

/// The sideways form of [`attach_rail`], for a single row of cards: the rail
/// lies under `body` and appears only while the row is wider than its place.
pub(crate) fn attach_rail_x(world: &mut World, root: Entity, body: Entity, prefix: &str) -> Entity {
    use crate::game_ui::tokens::Surface;
    use crate::game_ui::widgets::{Role, Swatch, Tone};
    let old_position = world
        .get::<Children>(root)
        .and_then(|children| children.iter().position(|child| child == body));
    let frame = world
        .spawn((
            Name::new(format!("{prefix} scroll viewport")),
            ChildOf(root),
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: px(4),
                flex_grow: 1.0,
                flex_shrink: 1.0,
                min_height: px(0),
                min_width: px(0),
                align_items: AlignItems::Stretch,
                ..default()
            },
        ))
        .id();
    world.entity_mut(body).insert(ChildOf(frame));
    if let Some(mut node) = world.get_mut::<Node>(body) {
        node.flex_grow = 1.0;
        node.flex_shrink = 1.0;
        node.min_width = px(0);
        node.min_height = px(0);
    }
    if let Some(position) = old_position {
        let mut order = world
            .get::<Children>(root)
            .map(|children| children.to_vec())
            .unwrap_or_default();
        order.retain(|entity| *entity != frame);
        order.insert(position.min(order.len()), frame);
        world.entity_mut(root).replace_children(&order);
    }
    let rail = world
        .spawn((
            Name::new(format!("{prefix} scroll rail")),
            ChildOf(frame),
            Hovered::default(),
            Swatch::Fill,
            Tone::new(Surface::Paper, Role::Sunken),
            Node {
                height: px(6),
                min_height: px(6),
                flex_shrink: 0.0,
                border_radius: BorderRadius::all(px(3)),
                display: Display::None,
                ..default()
            },
            BackgroundColor(Color::NONE),
            Scrollbar::new(body, ControlOrientation::Horizontal, 28.0),
        ))
        .id();
    world.spawn((
        ChildOf(rail),
        RailThumb,
        Hovered::default(),
        ScrollbarThumb {
            border_radius: BorderRadius::all(px(3)),
            ..default()
        },
        Swatch::Fill,
        Tone::new(Surface::Paper, Role::Control),
        BackgroundColor(Color::NONE),
    ));
    world.entity_mut(body).insert(RailOnlyX { rail });
    rail
}

/// How far (logical px) a body's content reaches past its right edge.
pub(crate) fn overflow_x(node: &ComputedNode) -> f32 {
    ((node.content_size().x - node.size().x) * node.inverse_scale_factor()).max(0.0)
}

fn page_button(
    world: &mut World,
    parent: Entity,
    target: Entity,
    direction: i32,
    prefix: &str,
    label: &str,
) -> Entity {
    let button = world
        .spawn((
            ChildOf(parent),
            Name::new(format!("{prefix}: {label}")),
            WidgetButton,
            PageScroll { target, direction },
            TabIndex(0),
            Hovered::default(),
            AccessibleLabel::new(label),
            Node {
                min_height: px(28),
                padding: UiRect::axes(px(8), px(5)),
                border: UiRect::all(px(1)),
                flex_shrink: 0.0,
                ..default()
            },
            BackgroundColor(BUTTON),
            BorderColor::all(Color::NONE),
        ))
        .id();
    world.spawn((
        ChildOf(button),
        ScrollTypography,
        Text::new(label),
        TextFont {
            font_size: FontSize::Px(12.0),
            ..default()
        },
        TextColor(TEXT),
    ));
    button
}
/// Logical pixels a body may fall short of its content before the scroll
/// rail and page buttons appear: layout rounds each panel row to whole
/// pixels, which alone can leave a list one pixel short.
const OVERFLOW_SLACK: f32 = 1.5;
fn bounds(node: &ComputedNode) -> (f32, f32) {
    let visible = (node.size().y - node.scrollbar_size.y).max(0.0) * node.inverse_scale_factor();
    let total = node.content_size().y * node.inverse_scale_factor();
    (visible, (total - visible).max(0.0))
}
fn page_activate(
    event: On<Activate>,
    buttons: Query<(&PageScroll, Has<InteractionDisabled>)>,
    mut scrolls: Query<(&ComputedNode, &mut ScrollPosition)>,
) {
    let Ok((control, false)) = buttons.get(event.entity) else {
        return;
    };
    let Ok((node, mut position)) = scrolls.get_mut(control.target) else {
        return;
    };
    let (visible, max) = bounds(node);
    position.y = (position.y + visible * 0.85 * control.direction as f32).clamp(0.0, max);
}
fn load_font(mut commands: Commands, fonts: Option<ResMut<Assets<Font>>>) {
    if let Some(mut fonts) = fonts {
        let font = Font::from_bytes(
            include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../assets/fonts/Barlow-Regular.ttf"
            ))
            .to_vec(),
        );
        commands.insert_resource(ScrollFont(fonts.add(font)));
    }
}
fn refresh(world: &mut World) {
    let rails: Vec<_> = world
        .query::<(Entity, &RailOnly, &ComputedNode)>()
        .iter(world)
        .map(|(body, rail, node)| (body, rail.rail, bounds(node).1 > OVERFLOW_SLACK))
        .collect();
    for (body, rail, overflowing) in rails {
        let display = if overflowing {
            Display::Flex
        } else {
            Display::None
        };
        if let Some(mut node) = world.get_mut::<Node>(rail)
            && node.display != display
        {
            node.display = display;
        }
        if !overflowing
            && let Some(mut position) = world.get_mut::<ScrollPosition>(body)
            && position.y != 0.0
        {
            position.y = 0.0;
        }
    }
    let rails: Vec<_> = world
        .query::<(Entity, &RailOnlyX, &ComputedNode)>()
        .iter(world)
        .map(|(body, rail, node)| (body, rail.rail, overflow_x(node) > OVERFLOW_SLACK))
        .collect();
    for (body, rail, overflowing) in rails {
        let display = if overflowing {
            Display::Flex
        } else {
            Display::None
        };
        if let Some(mut node) = world.get_mut::<Node>(rail)
            && node.display != display
        {
            node.display = display;
        }
        if !overflowing
            && let Some(mut position) = world.get_mut::<ScrollPosition>(body)
            && position.x != 0.0
        {
            position.x = 0.0;
        }
    }
    // The thumb darkens under the pointer and while dragged.
    let thumbs: Vec<_> = world
        .query_filtered::<(Entity, &Hovered, &ScrollbarDragState), With<RailThumb>>()
        .iter(world)
        .map(|(entity, hovered, drag)| (entity, hovered.get() || drag.dragging))
        .collect();
    for (entity, active) in thumbs {
        use crate::game_ui::tokens::Surface;
        use crate::game_ui::widgets::{Role, Tone};
        let tone = Tone::new(
            Surface::Paper,
            if active { Role::Caption } else { Role::Control },
        );
        if world.get::<Tone>(entity) != Some(&tone) {
            world.entity_mut(entity).insert(tone);
        }
    }
    let conditional: Vec<_> = world
        .query::<(Entity, &OverflowChrome, &ComputedNode)>()
        .iter(world)
        .map(|(body, chrome, node)| {
            (
                body,
                chrome.rail,
                chrome.footer,
                chrome.controls,
                // A body that is short by a rounded pixel still fits.
                bounds(node).1 > OVERFLOW_SLACK,
            )
        })
        .collect();
    for (body, rail, footer, controls, overflowing) in conditional {
        let display = if overflowing {
            Display::Flex
        } else {
            Display::None
        };
        for entity in [rail, footer] {
            if let Some(mut node) = world.get_mut::<Node>(entity)
                && node.display != display
            {
                node.display = display;
            }
        }
        for entity in controls {
            if let Some(mut tab) = world.get_mut::<TabIndex>(entity) {
                tab.set_if_neq(TabIndex(if overflowing { 0 } else { -1 }));
            }
            if !overflowing && world.resource::<InputFocus>().get() == Some(entity) {
                world.resource_mut::<InputFocus>().clear();
            }
        }
        if !overflowing
            && let Some(mut position) = world.get_mut::<ScrollPosition>(body)
            && position.y != 0.0
        {
            position.y = 0.0;
        }
    }
    let focus = world.resource::<InputFocus>().get();
    let controls: Vec<_> = world
        .query::<(Entity, &PageScroll, &Hovered)>()
        .iter(world)
        .map(|(entity, control, hovered)| {
            (entity, control.target, control.direction, hovered.get())
        })
        .collect();
    for (entity, target, direction, hovered) in controls {
        let disabled = world
            .get::<ComputedNode>(target)
            .zip(world.get::<ScrollPosition>(target))
            .is_none_or(|(node, position)| {
                let (_, max) = bounds(node);
                if direction < 0 {
                    position.y <= 0.5
                } else {
                    position.y >= max - 0.5
                }
            });
        let was_disabled = world.get::<InteractionDisabled>(entity).is_some();
        if disabled && !was_disabled {
            world.entity_mut(entity).insert(InteractionDisabled);
        } else if !disabled && was_disabled {
            world.entity_mut(entity).remove::<InteractionDisabled>();
        }
        if let Some(mut color) = world.get_mut::<BackgroundColor>(entity) {
            color.0 = if disabled {
                TRACK
            } else if hovered {
                Color::srgb(0.27, 0.32, 0.40)
            } else {
                BUTTON
            };
        }
        if let Some(mut border) = world.get_mut::<BorderColor>(entity) {
            *border = BorderColor::all(if focus == Some(entity) {
                GOLD
            } else {
                Color::NONE
            });
        }
    }
    let hints: Vec<_> = world
        .query::<(Entity, &ScrollHint)>()
        .iter(world)
        .map(|(entity, hint)| (entity, hint.0))
        .collect();
    for (entity, target) in hints {
        let label = world
            .get::<ComputedNode>(target)
            .zip(world.get::<ScrollPosition>(target))
            .map_or("Scroll for more", |(node, position)| {
                let (_, max) = bounds(node);
                if max <= 0.5 {
                    "All options visible"
                } else if position.y <= 0.5 {
                    "More options below"
                } else if position.y >= max - 0.5 {
                    "End of options"
                } else {
                    "Scroll for more"
                }
            });
        if let Some(mut text) = world.get_mut::<Text>(entity)
            && text.0 != label
        {
            text.0 = label.into();
        }
    }
    for (hovered, drag, mut color) in world
        .query_filtered::<(&Hovered, &ScrollbarDragState, &mut BackgroundColor), With<PanelThumb>>()
        .iter_mut(world)
    {
        color.0 = if hovered.get() || drag.dragging {
            GOLD
        } else {
            Color::srgb(0.71, 0.58, 0.31)
        };
    }
    let font = world
        .get_resource::<ScrollFont>()
        .map(|font| font.0.clone());
    if let Some(font) = font {
        for mut text in world
            .query_filtered::<&mut TextFont, With<ScrollTypography>>()
            .iter_mut(world)
        {
            if text.font != bevy::text::FontSource::Handle(font.clone()) {
                text.font = font.clone().into();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn conditional_chrome_tracks_overflow_and_removes_hidden_keyboard_targets() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        install(&mut app);
        let root = app.world_mut().spawn(Node::default()).id();
        let body = app
            .world_mut()
            .spawn((
                ChildOf(root),
                Node::default(),
                ScrollPosition::default(),
                ComputedNode {
                    size: Vec2::new(600.0, 400.0),
                    content_size: Vec2::new(600.0, 400.0),
                    inverse_scale_factor: 1.0,
                    ..default()
                },
            ))
            .id();
        let [up, down] =
            attach_when_needed(app.world_mut(), root, body, "Menu", ["Above", "Below"]);
        let chrome = app.world().get::<OverflowChrome>(body).unwrap();
        let (rail, footer) = (chrome.rail, chrome.footer);
        refresh(app.world_mut());
        for entity in [rail, footer] {
            assert_eq!(
                app.world().get::<Node>(entity).unwrap().display,
                Display::None
            );
        }
        assert_eq!(app.world().get::<TabIndex>(down), Some(&TabIndex(-1)));

        app.world_mut()
            .get_mut::<ComputedNode>(body)
            .unwrap()
            .content_size
            .y = 1400.0;
        refresh(app.world_mut());
        for entity in [rail, footer] {
            assert_eq!(
                app.world().get::<Node>(entity).unwrap().display,
                Display::Flex
            );
        }
        assert_eq!(app.world().get::<TabIndex>(down), Some(&TabIndex(0)));
        app.world_mut().trigger(Activate { entity: down });
        app.world_mut().flush();
        assert_eq!(app.world().get::<ScrollPosition>(body).unwrap().y, 340.0);

        app.world_mut()
            .resource_mut::<InputFocus>()
            .set(up, bevy::input_focus::FocusCause::Navigated);
        app.world_mut()
            .get_mut::<ComputedNode>(body)
            .unwrap()
            .size
            .y = 1600.0;
        refresh(app.world_mut());
        assert_eq!(
            app.world().get::<Node>(footer).unwrap().display,
            Display::None
        );
        assert_eq!(
            app.world().get::<Node>(rail).unwrap().display,
            Display::None
        );
        assert_eq!(app.world().get::<TabIndex>(up), Some(&TabIndex(-1)));
        assert_eq!(app.world().get::<ScrollPosition>(body).unwrap().y, 0.0);
        assert!(app.world().resource::<InputFocus>().get().is_none());
    }

    #[test]
    fn visible_page_controls_scroll_and_clamp_using_logical_pixels() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        install(&mut app);
        let root = app.world_mut().spawn(Node::default()).id();
        let body = app
            .world_mut()
            .spawn((
                ChildOf(root),
                Node::default(),
                ScrollPosition::default(),
                ComputedNode {
                    size: Vec2::new(600.0, 400.0),
                    content_size: Vec2::new(600.0, 1400.0),
                    inverse_scale_factor: 0.5,
                    ..default()
                },
            ))
            .id();
        let [up, down] = attach(
            app.world_mut(),
            root,
            body,
            "Test",
            ["More above", "More below"],
        );
        assert!(app.world().contains_resource::<UiScale>());
        assert_ne!(
            app.world().get::<ChildOf>(body).unwrap().parent(),
            root,
            "the area sits beside its visible rail"
        );
        app.world_mut().trigger(Activate { entity: down });
        app.world_mut().flush();
        assert_eq!(app.world().get::<ScrollPosition>(body).unwrap().y, 170.0);
        for _ in 0..9 {
            app.world_mut().trigger(Activate { entity: down });
            app.world_mut().flush();
        }
        assert_eq!(app.world().get::<ScrollPosition>(body).unwrap().y, 500.0);
        app.world_mut().trigger(Activate { entity: up });
        app.world_mut().flush();
        assert_eq!(app.world().get::<ScrollPosition>(body).unwrap().y, 330.0);
        refresh(app.world_mut());
        assert!(app.world().get::<InteractionDisabled>(up).is_none());
        assert!(app.world().get::<InteractionDisabled>(down).is_none());
    }
}
