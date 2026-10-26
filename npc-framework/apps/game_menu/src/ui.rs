//! The menu's look: the explorer modal's palette (`testbed_view::explorer_modal`),
//! large `ui_widgets` buttons, and their hover / pressed / focus / disabled
//! colours. Text uses Bevy's default font (Fira Mono, printable ASCII only).

use bevy::input_focus::InputFocus;
use bevy::input_focus::tab_navigation::TabIndex;
use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use bevy::text::{EditableText, TextCursorStyle};
use bevy::ui::{InteractionDisabled, Pressed};
use bevy::ui_widgets::{Button as WidgetButton, TextInput};

pub const BACKDROP: Color = Color::srgb(0.05, 0.06, 0.07);
pub const PANEL: Color = Color::srgb(0.08, 0.09, 0.11);
pub const PANEL_BORDER: Color = Color::srgb(0.35, 0.40, 0.48);
pub const FIELD: Color = Color::srgb(0.04, 0.05, 0.06);
pub const TITLE: Color = Color::srgb(0.97, 0.95, 0.85);
pub const TEXT: Color = Color::srgb(0.88, 0.90, 0.88);
pub const DIM: Color = Color::srgb(0.60, 0.63, 0.66);
pub const NOTE: Color = Color::srgb(0.95, 0.80, 0.45);
pub const ERROR: Color = Color::srgb(0.98, 0.55, 0.50);
pub const OK: Color = Color::srgb(0.60, 0.88, 0.62);

const BUTTON: Color = Color::srgb(0.20, 0.24, 0.30);
const BUTTON_HOVER: Color = Color::srgb(0.27, 0.32, 0.40);
const BUTTON_PRESSED: Color = Color::srgb(0.14, 0.17, 0.22);
const BUTTON_DISABLED: Color = Color::srgb(0.12, 0.13, 0.15);
const FOCUS_BORDER: Color = Color::srgb(0.95, 0.80, 0.45);

/// A menu button (for [`refresh_buttons`]).
#[derive(Component)]
pub struct MenuButton;

pub fn font(px: f32) -> TextFont {
    TextFont {
        font_size: FontSize::Px(px),
        ..default()
    }
}

/// Text in `color` at `px`.
pub fn label(text: impl Into<String>, px: f32, color: Color) -> impl Bundle {
    (Text::new(text), font(px), TextColor(color))
}

/// The full-window backdrop with a centred panel; returns the panel.
pub fn spawn_screen(commands: &mut Commands, scope: impl Bundle, title: &str) -> Entity {
    let mut panel = Entity::PLACEHOLDER;
    commands
        .spawn((
            scope,
            Node {
                width: percent(100),
                height: percent(100),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(BACKDROP),
            bevy::input_focus::tab_navigation::TabGroup::new(0),
        ))
        .with_children(|root| {
            panel = root
                .spawn((
                    Node {
                        flex_direction: FlexDirection::Column,
                        width: percent(90),
                        max_width: px(880),
                        max_height: percent(94),
                        padding: UiRect::all(px(24)),
                        row_gap: px(10),
                        border: UiRect::all(px(1)),
                        border_radius: BorderRadius::all(px(6)),
                        ..default()
                    },
                    BackgroundColor(PANEL),
                    BorderColor::all(PANEL_BORDER),
                ))
                .with_child(label(title, 26.0, TITLE))
                .id();
        });
    panel
}

/// A large button labelled `text` (with an optional dimmer second line),
/// disabled when `enabled` is false. `tab` orders keyboard focus.
pub fn button(
    parent: &mut ChildSpawnerCommands,
    text: &str,
    detail: Option<&str>,
    enabled: bool,
    tab: i32,
    action: impl Bundle,
) -> Entity {
    spawn_button(parent, text, detail, enabled, tab, action, None)
}

/// The width of a list button's name column (Testbed's scenario names).
pub const NAME_COLUMN_PX: f32 = 140.0;

/// A one-row list button: `text` in a `name_px`-wide column, `detail` beside
/// it; disabled when `enabled` is false.
pub fn list_button(
    parent: &mut ChildSpawnerCommands,
    (text, name_px): (&str, f32),
    detail: &str,
    enabled: bool,
    tab: i32,
    action: impl Bundle,
) -> Entity {
    spawn_button(
        parent,
        text,
        Some(detail),
        enabled,
        tab,
        action,
        Some(name_px),
    )
}

/// A one-line text field (a Bevy `TextInput`) holding `text`, `tab` in the
/// keyboard order.
pub fn text_field(text: impl AsRef<str>, tab: i32) -> impl Bundle {
    (
        TextInput,
        EditableText {
            allow_newlines: false,
            ..EditableText::new(text)
        },
        TextLayout::no_wrap(),
        font(16.0),
        TextColor(TEXT),
        TextCursorStyle::default(),
        TabIndex(tab),
        Node {
            width: percent(100),
            min_width: px(0),
            padding: UiRect::all(px(8)),
            border: UiRect::all(px(1)),
            border_radius: BorderRadius::all(px(4)),
            ..default()
        },
        BackgroundColor(FIELD),
        BorderColor::all(PANEL_BORDER),
    )
}

/// Enables or disables a button made by [`button`] (its labels dim).
pub fn set_enabled<F: bevy::ecs::query::QueryFilter>(
    commands: &mut Commands,
    button: Entity,
    enabled: bool,
    children: &Query<&Children>,
    colors: &mut Query<&mut TextColor, F>,
) {
    if enabled {
        commands.entity(button).remove::<InteractionDisabled>();
    } else {
        commands.entity(button).insert(InteractionDisabled);
    }
    if let Some(first) = children.get(button).ok().and_then(|c| c.first().copied())
        && let Ok(mut color) = colors.get_mut(first)
    {
        color.set_if_neq(TextColor(if enabled { TEXT } else { DIM }));
    }
}

fn spawn_button(
    parent: &mut ChildSpawnerCommands,
    text: &str,
    detail: Option<&str>,
    enabled: bool,
    tab: i32,
    action: impl Bundle,
    name_column: Option<f32>,
) -> Entity {
    let compact = name_column.is_some();
    let mut b = parent.spawn((
        MenuButton,
        WidgetButton,
        Hovered::default(),
        TabIndex(tab),
        Node {
            flex_direction: if compact {
                FlexDirection::Row
            } else {
                FlexDirection::Column
            },
            align_items: if compact {
                AlignItems::Center
            } else {
                AlignItems::Stretch
            },
            column_gap: px(12),
            flex_shrink: 0.0,
            padding: if compact {
                UiRect::axes(px(14), px(7))
            } else {
                UiRect::axes(px(18), px(10))
            },
            min_height: px(if compact { 38.0 } else { 48.0 }),
            justify_content: if compact {
                JustifyContent::FlexStart
            } else {
                JustifyContent::Center
            },
            border: UiRect::all(px(2)),
            border_radius: BorderRadius::all(px(4)),
            ..default()
        },
        BackgroundColor(BUTTON),
        BorderColor::all(BUTTON),
        action,
    ));
    if !enabled {
        b.insert(InteractionDisabled);
    }
    let color = if enabled { TEXT } else { DIM };
    b.with_children(|c| {
        if compact {
            c.spawn((
                label(text, 18.0, color),
                Node {
                    width: px(name_column.unwrap_or(NAME_COLUMN_PX)),
                    flex_shrink: 0.0,
                    ..default()
                },
            ));
        } else {
            c.spawn(label(text, 20.0, color));
        }
        if let Some(d) = detail {
            c.spawn((
                label(d, if compact { 13.0 } else { 14.0 }, DIM),
                Node {
                    flex_shrink: 1.0,
                    ..default()
                },
            ));
        }
    });
    b.id()
}

/// A small one-line button beside a list row (Open, Stop, Start detached).
pub fn small_button(
    parent: &mut ChildSpawnerCommands,
    text: &str,
    enabled: bool,
    tab: i32,
    action: impl Bundle,
) -> Entity {
    let mut b = parent.spawn((
        MenuButton,
        WidgetButton,
        Hovered::default(),
        TabIndex(tab),
        Node {
            align_items: AlignItems::Center,
            flex_shrink: 0.0,
            padding: UiRect::axes(px(12), px(7)),
            min_height: px(38.0),
            border: UiRect::all(px(2)),
            border_radius: BorderRadius::all(px(4)),
            ..default()
        },
        BackgroundColor(BUTTON),
        BorderColor::all(BUTTON),
        action,
    ));
    if !enabled {
        b.insert(InteractionDisabled);
    }
    b.with_child(label(text, 16.0, if enabled { TEXT } else { DIM }));
    b.id()
}

/// Lets a list button take the row's free width (its row also holds
/// [`small_button`]s).
pub fn grow(commands: &mut Commands, button: Entity) {
    commands.entity(button).queue(|mut e: EntityWorldMut| {
        if let Some(mut node) = e.get_mut::<Node>() {
            node.flex_grow = 1.0;
            node.flex_shrink = 1.0;
            node.min_width = px(0);
        }
    });
}

/// A row of buttons (Back / Save / Build ..).
pub fn row(parent: &mut ChildSpawnerCommands, children: impl FnOnce(&mut ChildSpawnerCommands)) {
    parent
        .spawn(Node {
            column_gap: px(10),
            margin: UiRect::top(px(8)),
            ..default()
        })
        .with_children(children);
}

/// What [`refresh_buttons`] reads and writes per button.
type ButtonState = (
    Entity,
    &'static mut BackgroundColor,
    &'static mut BorderColor,
    &'static Hovered,
    Has<Pressed>,
    Has<InteractionDisabled>,
);

/// Button colours from hover, pressed, focus and disabled.
pub fn refresh_buttons(focus: Res<InputFocus>, mut buttons: Query<ButtonState, With<MenuButton>>) {
    for (entity, mut bg, mut border, hovered, pressed, disabled) in &mut buttons {
        let fill = if disabled {
            BUTTON_DISABLED
        } else if pressed {
            BUTTON_PRESSED
        } else if hovered.0 {
            BUTTON_HOVER
        } else {
            BUTTON
        };
        let edge = if focus.get() == Some(entity) && !disabled {
            FOCUS_BORDER
        } else {
            fill
        };
        bg.set_if_neq(BackgroundColor(fill));
        border.set_if_neq(BorderColor::all(edge));
    }
}
