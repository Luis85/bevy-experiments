//! The shared widget kit of the island interface.
//!
//! Every widget is a plain Bevy UI node. A widget takes its colours from the
//! surface it sits on ([`Surface::Chrome`] or [`Surface::Paper`]) through the
//! role tokens, so the Dusk panel style is a table swap. Builders only build:
//! the caller owns the action component and its observer, as with the older
//! [`super::button`]. Nothing here reads or writes simulation state.
//!
//! Screens adopt the kit one at a time: the older `GameButton` controls keep
//! their own paint and are untouched by [`paint`].
use super::icon::{self, Icon};
use super::tokens::{self as t, Roles, Status, Surface, Theme, Type};
use super::{GameText, KeyFocus};
use bevy::input_focus::InputFocus;
use bevy::input_focus::tab_navigation::TabIndex;
use bevy::picking::Pickable;
use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use bevy::text::LineHeight;
use bevy::ui::{InteractionDisabled, Pressed, prelude::AccessibleLabel};
use bevy::ui_widgets::{
    Button as WidgetButton, Slider, SliderRange, SliderStep, SliderThumb, SliderValue,
};

/// Which token a text or an icon takes on its surface.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Role {
    Text,
    Secondary,
    Caption,
    Link,
    Status(Status),
    /// Honey on chrome; the progress colour on paper.
    Accent,
    /// Text on a filled button.
    OnFill,
    /// A decorative edge or divider.
    Edge,
    /// The fill of a raised element (a bar track, a card).
    Raised,
    /// Hover fills, key caps and badges.
    Sunken,
    /// The edge of a control.
    Control,
}

/// The colour of a text, an icon or a plain node, resolved from the theme
/// every frame. Parts of a [`Control`] are coloured by the control instead.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Tone {
    pub surface: Surface,
    pub role: Role,
}

impl Tone {
    pub const fn new(surface: Surface, role: Role) -> Self {
        Self { surface, role }
    }
    pub const fn text(surface: Surface) -> Self {
        Self::new(surface, Role::Text)
    }
    pub const fn secondary(surface: Surface) -> Self {
        Self::new(surface, Role::Secondary)
    }
    pub const fn caption(surface: Surface) -> Self {
        Self::new(surface, Role::Caption)
    }
    pub const fn status(surface: Surface, status: Status) -> Self {
        Self::new(surface, Role::Status(status))
    }
    pub fn color(self, theme: &Theme) -> Color {
        let roles = theme.roles(self.surface);
        match self.role {
            Role::Text => roles.ink,
            Role::Secondary => roles.ink_secondary,
            Role::Caption => roles.ink_caption,
            Role::Link => roles.link,
            Role::Status(status) => roles.status(status),
            Role::Accent => roles.progress,
            Role::OnFill => t::CREAM_50,
            Role::Edge => roles.border_soft,
            Role::Raised => roles.raised,
            Role::Sunken => roles.sunken,
            Role::Control => roles.border_control,
        }
    }
}

/// A plain node filled (or edged) with a tone: dividers, bar tracks, fills.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Swatch {
    Fill,
    Edge,
    Both,
}

/// The background of a panel, a bar or a card, resolved from the theme.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Skin {
    /// Parchment panel with its frame.
    Panel,
    /// Chrome bar or dock at 94% over the world.
    Chrome,
    /// Solid chrome: tooltips and toasts.
    ChromeSolid,
    /// A card on paper.
    Card,
    /// A boxed stat chip on paper.
    Boxed,
    /// A key cap.
    Key(Surface),
}

/// The kinds of interactive control.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    Primary,
    Secondary,
    Quiet,
    Danger,
    IconButton,
    DockTile,
    Segment,
    Tab,
    Row,
    Card,
    Chip,
    Toggle,
    /// A slider's track and thumb.
    Slider,
}

/// An interactive control of the kit. `on` is its lit, selected or checked
/// state; `marked` is a second emphasis (a total a building in hand draws on,
/// an unavailable card).
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Control {
    pub kind: Kind,
    pub surface: Surface,
    pub on: bool,
    pub marked: bool,
}

impl Control {
    pub const fn new(kind: Kind, surface: Surface) -> Self {
        Self {
            kind,
            surface,
            on: false,
            marked: false,
        }
    }
}

/// A part of a control, coloured by the control's state.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Part {
    Label,
    Secondary,
    /// A key cap inside the control: tinted on honey and moss fills.
    Key,
    /// The thumb of a toggle or a slider.
    Thumb,
    /// The travelled part of a slider's track.
    Travel,
}

/// Show a state without the pointer or the keyboard: the gallery only.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Forced {
    pub hovered: bool,
    pub pressed: bool,
    pub focused: bool,
}

/// What a control is doing, as far as its colours are concerned.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Interaction {
    pub hovered: bool,
    pub pressed: bool,
    pub disabled: bool,
    pub on: bool,
    pub marked: bool,
}

/// The colours of a control in one state.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Look {
    pub fill: Color,
    pub edge: Color,
    pub ink: Color,
    pub ink_secondary: Color,
    /// A one-pixel ring outside the edge: a selected card's second pixel.
    pub ring: Color,
}

fn darken(color: Color, by: f32) -> Color {
    let color = color.to_srgba();
    Color::srgb(
        color.red * (1.0 - by),
        color.green * (1.0 - by),
        color.blue * (1.0 - by),
    )
}

/// The colour table of section 7 of the spec, as one pure function.
pub fn look(kind: Kind, surface: Surface, state: Interaction, roles: &Roles) -> Look {
    let chrome = surface == Surface::Chrome;
    let none = Color::NONE;
    let plain = Look {
        fill: none,
        edge: none,
        ink: roles.ink,
        ink_secondary: roles.ink_secondary,
        ring: none,
    };
    let hover_fill = roles.sunken;
    let pressed_fill = if chrome {
        t::BARK_900
    } else {
        roles.border_soft
    };
    let buttonish = matches!(
        kind,
        Kind::Primary | Kind::Secondary | Kind::Quiet | Kind::Danger | Kind::IconButton
    );
    if state.disabled && buttonish {
        // Disabled keeps its place; the reason is beside it and in its tooltip.
        let muted = if chrome {
            t::BARK_600
        } else {
            roles.border_control
        };
        return Look {
            fill: if kind == Kind::Quiet {
                none
            } else {
                roles.raised
            },
            edge: if kind == Kind::Quiet {
                none
            } else {
                roles.border_soft
            },
            ink: muted,
            ink_secondary: muted,
            ring: none,
        };
    }
    match kind {
        Kind::Primary => Look {
            fill: if state.pressed {
                t::MOSS_800
            } else if state.hovered {
                t::MOSS_700
            } else {
                t::MOSS_600
            },
            ink: t::CREAM_50,
            ink_secondary: t::CREAM_50,
            ..plain
        },
        Kind::Danger => Look {
            fill: if state.hovered || state.pressed {
                darken(t::CRIT_INK, 0.08)
            } else {
                t::CRIT_INK
            },
            ink: t::CREAM_50,
            ink_secondary: t::CREAM_50,
            ..plain
        },
        Kind::Secondary | Kind::IconButton => {
            if state.on {
                return Look {
                    fill: t::HONEY_500,
                    edge: t::HONEY_500,
                    ink: t::INK_900,
                    ink_secondary: t::INK_900,
                    ring: none,
                };
            }
            Look {
                fill: if state.pressed {
                    pressed_fill
                } else if state.hovered {
                    hover_fill
                } else {
                    roles.raised
                },
                edge: roles.border_control,
                ..plain
            }
        }
        Kind::Quiet => Look {
            fill: if state.pressed {
                pressed_fill
            } else if state.hovered {
                hover_fill
            } else {
                none
            },
            // Secondary ink is too faint on the pressed fill: full ink there.
            ink: if state.pressed {
                roles.ink
            } else {
                roles.ink_secondary
            },
            ..plain
        },
        Kind::DockTile => {
            if state.disabled {
                Look {
                    ink: t::BARK_600,
                    ink_secondary: t::BARK_600,
                    ..plain
                }
            } else if state.on {
                Look {
                    fill: roles.active,
                    edge: roles.active,
                    ink: roles.on_active,
                    ink_secondary: roles.on_active,
                    ring: none,
                }
            } else if state.hovered || state.pressed {
                Look {
                    fill: if state.pressed {
                        pressed_fill
                    } else {
                        hover_fill
                    },
                    edge: roles.border_control,
                    ..plain
                }
            } else {
                Look {
                    ink_secondary: roles.ink,
                    ..plain
                }
            }
        }
        Kind::Segment => {
            if state.disabled {
                Look {
                    fill: roles.raised,
                    ink: if chrome {
                        t::BARK_600
                    } else {
                        roles.border_control
                    },
                    ink_secondary: if chrome {
                        t::BARK_600
                    } else {
                        roles.border_control
                    },
                    ..plain
                }
            } else if state.on {
                Look {
                    fill: roles.active,
                    ink: roles.on_active,
                    ink_secondary: roles.on_active,
                    ..plain
                }
            } else {
                Look {
                    fill: if state.hovered || state.pressed {
                        hover_fill
                    } else {
                        roles.raised
                    },
                    ..plain
                }
            }
        }
        Kind::Tab => Look {
            fill: if state.on { roles.raised } else { none },
            // The edge is the three-pixel line beneath the active tab.
            edge: if state.on { roles.selected_edge } else { none },
            ink: if state.on || state.hovered {
                roles.ink
            } else {
                roles.ink_secondary
            },
            ink_secondary: roles.ink_secondary,
            ring: none,
        },
        Kind::Row => Look {
            fill: if state.on {
                roles.selected
            } else if state.hovered || state.pressed {
                roles.raised
            } else {
                none
            },
            // The edge is the three-pixel mark at the left of a selected row.
            edge: if state.on { roles.selected_edge } else { none },
            ink: if state.disabled {
                roles.ink_caption
            } else {
                roles.ink
            },
            ..plain
        },
        Kind::Card => {
            if state.marked {
                // Unavailable: transparent, quiet content; the reason is a sentence.
                return Look {
                    edge: roles.border_soft,
                    ink: roles.ink_caption,
                    ink_secondary: roles.ink_caption,
                    ..plain
                };
            }
            Look {
                fill: if state.on {
                    roles.selected
                } else if state.hovered || state.pressed {
                    roles.sunken
                } else {
                    roles.raised
                },
                edge: if state.on {
                    roles.selected_edge
                } else {
                    roles.border_soft
                },
                ring: if state.on { roles.selected_edge } else { none },
                ..plain
            }
        }
        Kind::Chip => Look {
            fill: if chrome {
                if state.hovered || state.pressed {
                    roles.raised
                } else {
                    none
                }
            } else if state.hovered || state.pressed {
                roles.sunken
            } else {
                roles.raised
            },
            edge: if state.marked {
                t::HONEY_500
            } else if chrome {
                none
            } else {
                roles.border_soft
            },
            ..plain
        },
        Kind::Toggle => Look {
            fill: if state.on { t::MOSS_600 } else { roles.sunken },
            edge: if state.on {
                t::MOSS_600
            } else {
                roles.border_control
            },
            // The thumb.
            ink: if state.disabled {
                roles.border_control
            } else if state.on {
                t::CREAM_50
            } else {
                roles.ink_caption
            },
            ..plain
        },
        // Fill is the travelled part of the track; ink and edge are the thumb.
        Kind::Slider => Look {
            fill: if state.disabled {
                roles.border_soft
            } else {
                roles.progress
            },
            edge: if state.disabled {
                roles.border_soft
            } else if chrome {
                t::BARK_900
            } else {
                roles.progress
            },
            ink: if chrome { t::CREAM_50 } else { roles.surface },
            ..plain
        },
    }
}

// --- Text ----------------------------------------------------------------------

/// One line of text in a step of the type scale. Does not wrap.
pub fn label(
    world: &mut World,
    parent: Entity,
    value: impl Into<String>,
    step: Type,
    tone: Tone,
) -> Entity {
    let entity = world
        .spawn((
            ChildOf(parent),
            GameText,
            tone,
            Text::new(value),
            TextFont {
                font_size: FontSize::Px(step.size),
                ..default()
            },
            LineHeight::Px(step.line),
            TextColor(tone.color(&Theme::default())),
            TextLayout::default().with_linebreak(bevy::text::LineBreak::NoWrap),
            Node {
                flex_shrink: 0.0,
                ..default()
            },
        ))
        .id();
    // The weight comes from the type scale, never from a screen.
    if step.weight == t::Weight::SemiBold {
        world.entity_mut(entity).insert(super::Strong);
    }
    entity
}

/// Text that wraps inside its parent's width.
pub fn paragraph(
    world: &mut World,
    parent: Entity,
    value: impl Into<String>,
    step: Type,
    tone: Tone,
) -> Entity {
    let entity = label(world, parent, value, step, tone);
    world.entity_mut(entity).insert((
        TextLayout::default().with_linebreak(bevy::text::LineBreak::WordOrCharacter),
        // A definite width that may shrink: a text node measured at zero
        // width reports that measure as its content size and wraps per letter.
        Node {
            width: percent(100),
            flex_shrink: 1.0,
            min_width: px(0),
            ..default()
        },
    ));
    entity
}

pub fn set_text(world: &mut World, entity: Entity, value: &str) {
    if let Some(mut text) = world.get_mut::<Text>(entity)
        && text.0 != value
    {
        text.0 = value.into();
    }
}

pub fn show(world: &mut World, entity: Entity, shown: bool) {
    let display = if shown { Display::Flex } else { Display::None };
    if let Some(mut node) = world.get_mut::<Node>(entity)
        && node.display != display
    {
        node.display = display;
    }
}

// --- Surfaces --------------------------------------------------------------------

/// A parchment panel: surface, two-pixel frame, large radius, shadow, padding 12.
pub fn panel(mut node: Node) -> impl Bundle {
    node.border = UiRect::all(px(2));
    node.border_radius = BorderRadius::all(px(t::RADIUS_LG));
    if node.padding == UiRect::default() {
        node.padding = UiRect::all(px(t::PANEL_PADDING));
    }
    (
        node,
        Skin::Panel,
        BackgroundColor(t::PARCHMENT_100),
        BorderColor::all(t::BARK_700),
        t::shadow(t::SHADOW_2),
        Hovered::default(),
    )
}

/// A chrome bar or dock. The top bar passes a zero radius.
pub fn chrome(mut node: Node, radius: f32) -> impl Bundle {
    if node.border == UiRect::default() {
        node.border = UiRect::all(px(1));
    }
    node.border_radius = BorderRadius::all(px(radius));
    (
        node,
        Skin::Chrome,
        BackgroundColor(t::chrome_fill()),
        BorderColor::all(t::BARK_600),
        t::shadow(t::SHADOW_2),
        Hovered::default(),
    )
}

/// A panel header, 44 high: icon, title, and room for a Close button.
pub fn panel_header(world: &mut World, panel: Entity, picture: Icon, title: &str) -> Entity {
    let header = world
        .spawn((
            ChildOf(panel),
            Node {
                height: px(44.0 - t::PANEL_PADDING),
                min_height: px(44.0 - t::PANEL_PADDING),
                align_items: AlignItems::Center,
                column_gap: px(8),
                flex_shrink: 0.0,
                ..default()
            },
        ))
        .id();
    icon::icon(world, header, picture, 20.0, Tone::text(Surface::Paper));
    let title = label(world, header, title, t::HEADING, Tone::text(Surface::Paper));
    world.entity_mut(title).insert(Node {
        flex_grow: 1.0,
        ..default()
    });
    header
}

/// A one-pixel divider in the surface's soft edge colour.
pub fn divider(world: &mut World, parent: Entity, surface: Surface, vertical: bool) -> Entity {
    world
        .spawn((
            ChildOf(parent),
            Swatch::Fill,
            Tone::new(surface, Role::Edge),
            Node {
                width: if vertical { px(1) } else { percent(100) },
                height: if vertical { px(24) } else { px(1) },
                flex_shrink: 0.0,
                ..default()
            },
            BackgroundColor(Color::NONE),
        ))
        .id()
}

/// A static card on paper. For a selectable card use [`card_button`].
pub fn card(world: &mut World, parent: Entity, padding: f32) -> Entity {
    world
        .spawn((
            ChildOf(parent),
            Skin::Card,
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: px(4),
                padding: UiRect::all(px(padding)),
                border: UiRect::all(px(1)),
                border_radius: BorderRadius::all(px(t::RADIUS_MD)),
                flex_shrink: 0.0,
                ..default()
            },
            BackgroundColor(t::PARCHMENT_200),
            BorderColor::all(t::BORDER_SOFT),
        ))
        .id()
}

// --- Controls --------------------------------------------------------------------

/// How a control joins the keyboard path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reach {
    /// A stop in its panel's own Tab group; a click focuses it.
    Tab(i32),
    /// A stop in the session ring that a click does not focus, so Space
    /// still pauses after a click on the bar or a dock.
    Ring,
}

/// Spawn the base of any control: semantics, hover, focus reach and paint.
#[allow(clippy::too_many_arguments)]
pub fn control(
    world: &mut World,
    parent: Entity,
    name: &str,
    kind: Kind,
    surface: Surface,
    reach: Reach,
    node: Node,
    action: impl Bundle,
) -> Entity {
    let entity = world
        .spawn((
            ChildOf(parent),
            Name::new(format!("Game: {name}")),
            Control::new(kind, surface),
            WidgetButton,
            Hovered::default(),
            AccessibleLabel::new(name),
            node,
            BackgroundColor(Color::NONE),
            BorderColor::all(Color::NONE),
            Outline::new(px(2), px(2), Color::NONE),
            action,
        ))
        .id();
    match reach {
        Reach::Tab(index) => {
            world.entity_mut(entity).insert(TabIndex(index));
        }
        Reach::Ring => {
            world.entity_mut(entity).insert(KeyFocus);
        }
    }
    entity
}

/// Size of a text button.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Size {
    Small,
    Medium,
    Large,
}

impl Size {
    pub const fn height(self) -> f32 {
        match self {
            Self::Small => t::BUTTON_SMALL,
            Self::Medium => t::BUTTON_MEDIUM,
            Self::Large => t::BUTTON_LARGE,
        }
    }
}

/// What a text button shows.
#[derive(Clone, Copy, Debug)]
pub struct ButtonSpec<'a> {
    pub label: &'a str,
    pub kind: Kind,
    pub surface: Surface,
    pub size: Size,
    pub icon: Option<Icon>,
    /// The binding, shown as a key cap at the right. Never part of the label.
    pub key: Option<&'a str>,
    pub reach: Reach,
}

impl<'a> ButtonSpec<'a> {
    pub const fn new(label: &'a str, kind: Kind, surface: Surface) -> Self {
        Self {
            label,
            kind,
            surface,
            size: Size::Medium,
            icon: None,
            key: None,
            reach: Reach::Tab(0),
        }
    }
    pub const fn size(mut self, size: Size) -> Self {
        self.size = size;
        self
    }
    pub const fn icon(mut self, icon: Icon) -> Self {
        self.icon = Some(icon);
        self
    }
    pub const fn key(mut self, key: &'a str) -> Self {
        self.key = Some(key);
        self
    }
    pub const fn reach(mut self, reach: Reach) -> Self {
        self.reach = reach;
        self
    }
}

/// A button: primary, secondary, quiet or danger, on either surface.
pub fn button(world: &mut World, parent: Entity, spec: ButtonSpec, action: impl Bundle) -> Entity {
    let height = spec.size.height();
    let entity = control(
        world,
        parent,
        spec.label,
        spec.kind,
        spec.surface,
        spec.reach,
        Node {
            height: px(height),
            min_height: px(height),
            min_width: px(t::TARGET_MIN.1),
            padding: UiRect::horizontal(px(if spec.size == Size::Small { 10 } else { 14 })),
            column_gap: px(6),
            border: UiRect::all(px(1)),
            border_radius: BorderRadius::all(px(t::RADIUS_MD)),
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            flex_shrink: 0.0,
            ..default()
        },
        action,
    );
    if let Some(picture) = spec.icon {
        let picture = icon::icon(world, entity, picture, 16.0, Tone::text(spec.surface));
        world.entity_mut(picture).insert(Part::Label);
    }
    let step = if spec.size == Size::Large {
        t::SUBHEADING
    } else {
        t::BODY
    };
    let text = label(world, entity, spec.label, step, Tone::text(spec.surface));
    world.entity_mut(text).insert(Part::Label);
    if let Some(key) = spec.key {
        let cap = keycap(world, entity, spec.surface, key);
        world.entity_mut(cap).insert(Part::Key);
    }
    entity
}

/// A 32 x 32 icon button. Always give it a [`super::tooltip::Tooltip`] with
/// its name and key.
pub fn icon_button(
    world: &mut World,
    parent: Entity,
    name: &str,
    picture: Icon,
    surface: Surface,
    reach: Reach,
    action: impl Bundle,
) -> Entity {
    let entity = control(
        world,
        parent,
        name,
        Kind::IconButton,
        surface,
        reach,
        Node {
            width: px(t::ICON_BUTTON),
            height: px(t::ICON_BUTTON),
            min_width: px(t::ICON_BUTTON),
            border: UiRect::all(px(1)),
            border_radius: BorderRadius::all(px(t::RADIUS_MD)),
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            flex_shrink: 0.0,
            ..default()
        },
        action,
    );
    let picture = icon::icon(world, entity, picture, 16.0, Tone::text(surface));
    world.entity_mut(picture).insert(Part::Label);
    entity
}

/// A key cap: 18 high, one cap per binding ("Ctrl+S", "Click").
pub fn keycap(world: &mut World, parent: Entity, surface: Surface, key: &str) -> Entity {
    let cap = world
        .spawn((
            ChildOf(parent),
            Skin::Key(surface),
            Pickable::IGNORE,
            Node {
                height: px(t::KEY_CAP),
                min_width: px(t::KEY_CAP),
                padding: UiRect::horizontal(px(4)),
                border: UiRect::all(px(1)),
                border_radius: BorderRadius::all(px(t::RADIUS_SM)),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                flex_shrink: 0.0,
                ..default()
            },
            BackgroundColor(Color::NONE),
            BorderColor::all(Color::NONE),
        ))
        .id();
    let text = label(world, cap, key, t::CAPTION, Tone::secondary(surface));
    world.entity_mut(text).insert(Pickable::IGNORE);
    cap
}

/// A dock tile: icon over a caption label, key cap in the top right corner
/// and an optional count badge. `width` is 64, or 56 at the Standard layout.
pub struct DockTile {
    pub entity: Entity,
    pub label: Entity,
    pub key: Option<Entity>,
    pub badge: Entity,
    pub badge_text: Entity,
}

pub fn dock_tile(
    world: &mut World,
    parent: Entity,
    name: &str,
    picture: Icon,
    key: Option<&str>,
    action: impl Bundle,
) -> DockTile {
    let entity = control(
        world,
        parent,
        name,
        Kind::DockTile,
        Surface::Chrome,
        Reach::Ring,
        Node {
            width: px(t::DOCK_TILE.0),
            height: px(t::DOCK_TILE.1),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            row_gap: px(1),
            border: UiRect::all(px(1)),
            border_radius: BorderRadius::all(px(t::RADIUS_MD)),
            flex_shrink: 0.0,
            ..default()
        },
        action,
    );
    let picture = icon::icon(world, entity, picture, 20.0, Tone::text(Surface::Chrome));
    world.entity_mut(picture).insert(Part::Label);
    let text = label(
        world,
        entity,
        name,
        t::CAPTION,
        Tone::secondary(Surface::Chrome),
    );
    world.entity_mut(text).insert(Part::Secondary);
    let key = key.map(|key| {
        let cap = keycap(world, entity, Surface::Chrome, key);
        let mut node = world.get_mut::<Node>(cap).expect("key cap");
        node.position_type = PositionType::Absolute;
        node.top = px(1);
        node.right = px(1);
        node.height = px(14);
        node.min_width = px(14);
        node.padding = UiRect::horizontal(px(2));
        world.entity_mut(cap).insert(Part::Key);
        cap
    });
    let (badge, badge_text) = count_badge(world, entity, "");
    {
        let mut node = world.get_mut::<Node>(badge).expect("badge");
        node.position_type = PositionType::Absolute;
        node.top = px(-7);
        node.right = px(-5);
        node.display = Display::None;
    }
    DockTile {
        entity,
        label: text,
        key,
        badge,
        badge_text,
    }
}

/// A row of joined segments; one is on. 30 high on chrome, 26 on paper.
pub fn segmented(world: &mut World, parent: Entity, surface: Surface) -> Entity {
    // The container's fill shows through the one-pixel gaps as separators.
    world
        .spawn((
            ChildOf(parent),
            Swatch::Both,
            Tone::new(
                surface,
                if surface == Surface::Chrome {
                    Role::Edge
                } else {
                    Role::Control
                },
            ),
            Node {
                border: UiRect::all(px(1)),
                border_radius: BorderRadius::all(px(t::RADIUS_MD)),
                column_gap: px(1),
                overflow: Overflow::clip(),
                flex_shrink: 0.0,
                ..default()
            },
            BackgroundColor(Color::NONE),
            BorderColor::all(Color::NONE),
        ))
        .id()
}

#[allow(clippy::too_many_arguments)]
pub fn segment(
    world: &mut World,
    group: Entity,
    name: &str,
    picture: Option<Icon>,
    text: Option<&str>,
    key: Option<&str>,
    reach: Reach,
    action: impl Bundle,
) -> Entity {
    let surface = world
        .get::<Tone>(group)
        .map_or(Surface::Chrome, |tone| tone.surface);
    let height = if surface == Surface::Chrome {
        t::SEGMENT_CHROME
    } else {
        t::SEGMENT_PAPER
    };
    let entity = control(
        world,
        group,
        name,
        Kind::Segment,
        surface,
        reach,
        Node {
            height: px(height),
            min_width: px(t::TARGET_MIN.1),
            padding: UiRect::horizontal(px(8)),
            column_gap: px(6),
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            flex_shrink: 0.0,
            ..default()
        },
        action,
    );
    if let Some(picture) = picture {
        let picture = icon::icon(world, entity, picture, 16.0, Tone::text(surface));
        world.entity_mut(picture).insert(Part::Label);
    }
    if let Some(text) = text {
        let step = if surface == Surface::Chrome {
            t::BODY
        } else {
            t::DENSE
        };
        let text = label(world, entity, text, step, Tone::text(surface));
        world.entity_mut(text).insert(Part::Label);
    }
    if let Some(key) = key {
        let cap = keycap(world, entity, surface, key);
        world.entity_mut(cap).insert(Part::Key);
    }
    entity
}

/// A tab strip on paper: tabs 36 high over a one-pixel line.
pub fn tabs(world: &mut World, parent: Entity) -> Entity {
    world
        .spawn((
            ChildOf(parent),
            Swatch::Edge,
            Tone::new(Surface::Paper, Role::Edge),
            Node {
                border: UiRect::bottom(px(1)),
                column_gap: px(2),
                flex_shrink: 0.0,
                ..default()
            },
            BorderColor::all(Color::NONE),
            BackgroundColor(Color::NONE),
        ))
        .id()
}

pub fn tab(
    world: &mut World,
    strip: Entity,
    name: &str,
    picture: Option<Icon>,
    reach: Reach,
    action: impl Bundle,
) -> Entity {
    let entity = control(
        world,
        strip,
        name,
        Kind::Tab,
        Surface::Paper,
        reach,
        Node {
            height: px(t::TAB),
            padding: UiRect::horizontal(px(8)),
            column_gap: px(5),
            border: UiRect::bottom(px(3)),
            border_radius: BorderRadius::top(px(t::RADIUS_MD)),
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            flex_grow: 1.0,
            flex_shrink: 1.0,
            min_width: px(0),
            ..default()
        },
        action,
    );
    if let Some(picture) = picture {
        let picture = icon::icon(
            world,
            entity,
            picture,
            14.0,
            Tone::secondary(Surface::Paper),
        );
        world.entity_mut(picture).insert(Part::Label);
    }
    let text = label(
        world,
        entity,
        name,
        t::DENSE,
        Tone::secondary(Surface::Paper),
    );
    world.entity_mut(text).insert(Part::Label);
    entity
}

/// A list row: 28 high with one line, 44 with two. The caller adds content.
pub fn list_row(
    world: &mut World,
    parent: Entity,
    name: &str,
    two_lines: bool,
    reach: Reach,
    action: impl Bundle,
) -> Entity {
    let height = if two_lines {
        t::ROW_TWO_LINES
    } else {
        t::ROW_ONE_LINE
    };
    control(
        world,
        parent,
        name,
        Kind::Row,
        Surface::Paper,
        reach,
        Node {
            min_height: px(height),
            padding: UiRect::new(px(7), px(8), px(2), px(2)),
            column_gap: px(8),
            border: UiRect::left(px(3)),
            border_radius: BorderRadius::all(px(t::RADIUS_SM)),
            align_items: AlignItems::Center,
            flex_shrink: 0.0,
            ..default()
        },
        action,
    )
}

/// A selectable card on paper.
pub fn card_button(
    world: &mut World,
    parent: Entity,
    name: &str,
    padding: f32,
    reach: Reach,
    action: impl Bundle,
) -> Entity {
    control(
        world,
        parent,
        name,
        Kind::Card,
        Surface::Paper,
        reach,
        Node {
            flex_direction: FlexDirection::Column,
            row_gap: px(4),
            padding: UiRect::all(px(padding)),
            border: UiRect::all(px(1)),
            border_radius: BorderRadius::all(px(t::RADIUS_MD)),
            flex_shrink: 0.0,
            ..default()
        },
        action,
    )
}

/// The parts of a stat chip a refresh needs to reach.
#[derive(Clone, Copy, Debug)]
pub struct StatChip {
    pub entity: Entity,
    pub value: Entity,
    /// The status shape or trend arrow after the number; hidden until shown.
    pub mark: Entity,
}

/// A stat chip: icon 16, number, optional unit. 28 high on chrome, 24 in a
/// box on paper. Clickable: the caller's action opens the matching page.
#[allow(clippy::too_many_arguments)]
pub fn stat_chip(
    world: &mut World,
    parent: Entity,
    name: &str,
    picture: Icon,
    value: &str,
    unit: Option<&str>,
    surface: Surface,
    reach: Reach,
    action: impl Bundle,
) -> StatChip {
    let chrome = surface == Surface::Chrome;
    let entity = control(
        world,
        parent,
        name,
        Kind::Chip,
        surface,
        reach,
        Node {
            height: px(if chrome {
                t::CHIP_CHROME
            } else {
                t::CHIP_PAPER
            }),
            padding: UiRect::horizontal(px(6)),
            column_gap: px(5),
            border: UiRect::all(px(1)),
            border_radius: BorderRadius::all(px(t::RADIUS_MD)),
            align_items: AlignItems::Center,
            flex_shrink: 0.0,
            ..default()
        },
        action,
    );
    icon::icon(world, entity, picture, 16.0, Tone::secondary(surface));
    let value = label(world, entity, value, t::FIGURE, Tone::text(surface));
    if let Some(unit) = unit {
        label(world, entity, unit, t::CAPTION, Tone::secondary(surface));
    }
    let mark = world
        .spawn((
            ChildOf(entity),
            Pickable::IGNORE,
            Node {
                display: Display::None,
                ..default()
            },
        ))
        .id();
    StatChip {
        entity,
        value,
        mark,
    }
}

/// Replace a chip's mark: a status shape, a trend arrow, or nothing.
pub fn set_mark(world: &mut World, mark: Entity, wanted: Option<(Icon, Status)>, surface: Surface) {
    #[derive(Component, PartialEq, Clone, Copy)]
    struct Shown(Icon, Status);
    let current = world.get::<Shown>(mark).copied();
    let next = wanted.map(|(icon, status)| Shown(icon, status));
    if current == next {
        return;
    }
    world.entity_mut(mark).despawn_children();
    match next {
        Some(shown) => {
            icon::icon(world, mark, shown.0, 12.0, Tone::status(surface, shown.1));
            world.entity_mut(mark).insert(shown);
            show(world, mark, true);
        }
        None => {
            world.entity_mut(mark).remove::<Shown>();
            show(world, mark, false);
        }
    }
}

/// A badge: 20 high pill with a status shape and a word or a count.
pub fn badge(
    world: &mut World,
    parent: Entity,
    surface: Surface,
    status: Status,
    text: &str,
) -> Entity {
    let entity = world
        .spawn((
            ChildOf(parent),
            Swatch::Fill,
            Tone::new(surface, Role::Sunken),
            Node {
                height: px(20),
                padding: UiRect::horizontal(px(6)),
                column_gap: px(4),
                border_radius: BorderRadius::all(px(10)),
                align_items: AlignItems::Center,
                flex_shrink: 0.0,
                ..default()
            },
            BackgroundColor(Color::NONE),
        ))
        .id();
    icon::icon(
        world,
        entity,
        Icon::status(status),
        12.0,
        Tone::status(surface, status),
    );
    // The shape carries the colour; the word stays in ink (amber is never text).
    label(world, entity, text, t::CAPTION, Tone::text(surface));
    entity
}

/// A count badge for a tile: 16 high, honey with dark ink. Returns the pill
/// and its text.
pub fn count_badge(world: &mut World, parent: Entity, text: &str) -> (Entity, Entity) {
    let entity = world
        .spawn((
            ChildOf(parent),
            Pickable::IGNORE,
            Node {
                height: px(16),
                min_width: px(16),
                padding: UiRect::horizontal(px(4)),
                border_radius: BorderRadius::all(px(8)),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                flex_shrink: 0.0,
                ..default()
            },
            BackgroundColor(t::HONEY_500),
        ))
        .id();
    let text = world
        .spawn((
            ChildOf(entity),
            GameText,
            Pickable::IGNORE,
            Text::new(text),
            TextFont {
                font_size: FontSize::Px(t::CAPTION.size),
                ..default()
            },
            LineHeight::Px(t::CAPTION.line),
            TextColor(t::INK_900),
            TextLayout::default().with_linebreak(bevy::text::LineBreak::NoWrap),
        ))
        .id();
    (entity, text)
}

/// The fill of a bar: its width is the value.
#[derive(Clone, Copy, Debug)]
pub struct Bar {
    pub entity: Entity,
    pub fill: Entity,
    pub value: Option<Entity>,
}

fn bar_track(world: &mut World, parent: Entity, surface: Surface, height: f32) -> (Entity, Entity) {
    let track = world
        .spawn((
            ChildOf(parent),
            Skin::Boxed,
            Tone::new(surface, Role::Raised),
            Node {
                height: px(height),
                flex_grow: 1.0,
                min_width: px(40),
                border: UiRect::all(px(1)),
                border_radius: BorderRadius::all(px(t::RADIUS_SM)),
                overflow: Overflow::clip(),
                ..default()
            },
            BackgroundColor(Color::NONE),
            BorderColor::all(Color::NONE),
        ))
        .id();
    if surface == Surface::Chrome {
        world.entity_mut(track).insert(Skin::Key(Surface::Chrome));
    }
    let fill = world
        .spawn((
            ChildOf(track),
            Swatch::Fill,
            Tone::new(surface, Role::Accent),
            Node {
                width: percent(0),
                height: percent(100),
                ..default()
            },
            BackgroundColor(Color::NONE),
        ))
        .id();
    (track, fill)
}

/// A progress bar, 10 high, with its value text at the right ("200 / 600").
pub fn progress_bar(
    world: &mut World,
    parent: Entity,
    surface: Surface,
    fraction: f32,
    value: Option<&str>,
) -> Bar {
    let entity = world
        .spawn((
            ChildOf(parent),
            Node {
                align_items: AlignItems::Center,
                column_gap: px(8),
                flex_shrink: 0.0,
                ..default()
            },
        ))
        .id();
    let (_, fill) = bar_track(world, entity, surface, 10.0);
    let value = value.map(|value| label(world, entity, value, t::DENSE, Tone::secondary(surface)));
    let bar = Bar {
        entity,
        fill,
        value,
    };
    set_bar(world, bar, fraction, None);
    bar
}

/// A row of progress pips: steps that are done are filled, the current one
/// is a honey ring, the rest are hollow. Shape, not colour alone, tells
/// them apart. Fill it with [`set_pips`].
pub fn pips(world: &mut World, parent: Entity) -> Entity {
    world
        .spawn((
            ChildOf(parent),
            Node {
                align_items: AlignItems::Center,
                column_gap: px(5),
                flex_shrink: 0.0,
                ..default()
            },
        ))
        .id()
}

/// What a pip row last showed: done, current (or none), total.
#[derive(Component, Clone, Copy, PartialEq, Eq)]
struct PipsShown(u32, Option<u32>, u32);

/// `done` filled pips of `total`; `current` (0-based) is ringed when it is
/// not done. Rebuilt only when one of the three changes.
pub fn set_pips(
    world: &mut World,
    row: Entity,
    surface: Surface,
    done: u32,
    current: Option<u32>,
    total: u32,
) {
    let wanted = PipsShown(done, current, total);
    if world.get::<PipsShown>(row) == Some(&wanted) {
        return;
    }
    world.entity_mut(row).despawn_children();
    for index in 0..total.min(12) {
        let (border, tone, swatch) = if index < done {
            (1.0, Tone::new(surface, Role::Accent), Swatch::Both)
        } else if Some(index) == current {
            (2.0, Tone::new(surface, Role::Accent), Swatch::Edge)
        } else {
            (1.0, Tone::new(surface, Role::Control), Swatch::Edge)
        };
        world.spawn((
            ChildOf(row),
            Node {
                width: px(10),
                height: px(10),
                border: UiRect::all(px(border)),
                border_radius: BorderRadius::all(px(5)),
                flex_shrink: 0.0,
                ..default()
            },
            tone,
            swatch,
            BackgroundColor(Color::NONE),
            BorderColor::all(Color::NONE),
        ));
    }
    world.entity_mut(row).insert(wanted);
}

/// A need bar, 8 high: icon and label before it, a triangle or a diamond
/// after it when the need is not fine.
#[derive(Clone, Copy, Debug)]
pub struct NeedBar {
    pub bar: Bar,
    pub mark: Entity,
}

pub fn need_bar(
    world: &mut World,
    parent: Entity,
    surface: Surface,
    picture: Icon,
    name: &str,
    fraction: f32,
    status: Status,
) -> NeedBar {
    let entity = world
        .spawn((
            ChildOf(parent),
            Node {
                align_items: AlignItems::Center,
                column_gap: px(6),
                flex_shrink: 0.0,
                ..default()
            },
        ))
        .id();
    icon::icon(world, entity, picture, 16.0, Tone::secondary(surface));
    let name = label(world, entity, name, t::DENSE, Tone::text(surface));
    world.entity_mut(name).insert(Node {
        width: px(52),
        flex_shrink: 0.0,
        ..default()
    });
    let (_, fill) = bar_track(world, entity, surface, 8.0);
    let mark = world
        .spawn((
            ChildOf(entity),
            Node {
                width: px(12),
                height: px(12),
                flex_shrink: 0.0,
                ..default()
            },
        ))
        .id();
    let bar = NeedBar {
        bar: Bar {
            entity,
            fill,
            value: None,
        },
        mark,
    };
    set_need(world, bar, fraction, status, surface);
    bar
}

pub fn set_bar(world: &mut World, bar: Bar, fraction: f32, value: Option<&str>) {
    let width = percent(fraction.clamp(0.0, 1.0) * 100.0);
    if let Some(mut node) = world.get_mut::<Node>(bar.fill)
        && node.width != width
    {
        node.width = width;
    }
    if let (Some(entity), Some(value)) = (bar.value, value) {
        set_text(world, entity, value);
    }
}

pub fn set_need(world: &mut World, need: NeedBar, fraction: f32, status: Status, surface: Surface) {
    set_bar(world, need.bar, fraction, None);
    let tone = Tone::status(surface, status);
    if world.get::<Tone>(need.bar.fill) != Some(&tone) {
        world.entity_mut(need.bar.fill).insert(tone);
    }
    let wanted =
        matches!(status, Status::Warn | Status::Crit).then_some((Icon::status(status), status));
    #[derive(Component, PartialEq, Clone, Copy)]
    struct Marked(Status);
    let current = world.get::<Marked>(need.mark).map(|marked| marked.0);
    if current != wanted.map(|(_, status)| status) {
        world.entity_mut(need.mark).despawn_children();
        world.entity_mut(need.mark).remove::<Marked>();
        if let Some((picture, status)) = wanted {
            icon::icon(
                world,
                need.mark,
                picture,
                12.0,
                Tone::status(surface, status),
            );
            world.entity_mut(need.mark).insert(Marked(status));
        }
    }
}

/// A toggle: a 36 x 20 pill. Position, not colour, shows the state; the
/// label beside it names the thing, not the state.
pub fn toggle(
    world: &mut World,
    parent: Entity,
    name: &str,
    surface: Surface,
    on: bool,
    reach: Reach,
    action: impl Bundle,
) -> Entity {
    let entity = control(
        world,
        parent,
        name,
        Kind::Toggle,
        surface,
        reach,
        Node {
            width: px(36),
            height: px(20),
            padding: UiRect::all(px(2)),
            border: UiRect::all(px(1)),
            border_radius: BorderRadius::all(px(10)),
            align_items: AlignItems::Center,
            flex_shrink: 0.0,
            ..default()
        },
        action,
    );
    world.spawn((
        ChildOf(entity),
        Part::Thumb,
        Pickable::IGNORE,
        Node {
            width: px(14),
            height: px(14),
            border_radius: BorderRadius::all(px(7)),
            ..default()
        },
        BackgroundColor(Color::NONE),
    ));
    world.get_mut::<Control>(entity).expect("toggle").on = on;
    entity
}

/// A slider: track 6 high, round thumb 16, value text at the right. Left and
/// Right step it; the caller observes `ValueChange<f32>`.
#[derive(Clone, Copy, Debug)]
pub struct SliderParts {
    pub entity: Entity,
    pub value: Entity,
}

#[allow(clippy::too_many_arguments)]
pub fn slider(
    world: &mut World,
    parent: Entity,
    name: &str,
    surface: Surface,
    range: (f32, f32),
    step: f32,
    value: f32,
    text: &str,
    reach: Reach,
    action: impl Bundle,
) -> SliderParts {
    let row = world
        .spawn((
            ChildOf(parent),
            Node {
                align_items: AlignItems::Center,
                column_gap: px(10),
                flex_shrink: 0.0,
                ..default()
            },
        ))
        .id();
    let entity = world
        .spawn((
            ChildOf(row),
            Name::new(format!("Game: {name}")),
            Control::new(Kind::Slider, surface),
            Slider::default(),
            SliderRange::new(range.0, range.1),
            SliderStep(step),
            SliderValue(value),
            Hovered::default(),
            AccessibleLabel::new(name),
            Node {
                width: px(132),
                min_width: px(132),
                height: px(t::BUTTON_SMALL),
                align_items: AlignItems::Center,
                border_radius: BorderRadius::all(px(t::RADIUS_SM)),
                flex_shrink: 0.0,
                ..default()
            },
            Outline::new(px(2), px(2), Color::NONE),
            action,
        ))
        .id();
    match reach {
        Reach::Tab(index) => {
            world.entity_mut(entity).insert(TabIndex(index));
        }
        Reach::Ring => {
            world.entity_mut(entity).insert(KeyFocus);
        }
    }
    // The visible track, six pixels high, behind the thumb.
    world.spawn((
        ChildOf(entity),
        Skin::Boxed,
        Tone::new(surface, Role::Raised),
        Pickable::IGNORE,
        Node {
            position_type: PositionType::Absolute,
            left: px(0),
            right: px(0),
            height: px(6),
            border: UiRect::all(px(1)),
            border_radius: BorderRadius::all(px(3)),
            ..default()
        },
        BackgroundColor(Color::NONE),
        BorderColor::all(Color::NONE),
    ));
    world.spawn((
        ChildOf(entity),
        Part::Travel,
        Pickable::IGNORE,
        Node {
            position_type: PositionType::Absolute,
            left: px(0),
            width: px(0),
            height: px(6),
            border_radius: BorderRadius::all(px(3)),
            ..default()
        },
        BackgroundColor(Color::NONE),
    ));
    world.spawn((
        ChildOf(entity),
        SliderThumb,
        Part::Thumb,
        Node {
            position_type: PositionType::Absolute,
            width: px(16),
            height: px(16),
            border: UiRect::all(px(2)),
            border_radius: BorderRadius::all(px(8)),
            ..default()
        },
        BackgroundColor(Color::NONE),
        BorderColor::all(Color::NONE),
    ));
    let value = label(world, row, text, t::DENSE, Tone::secondary(surface));
    SliderParts { entity, value }
}

/// Keep slider thumbs where their values are.
pub(super) fn place_thumbs(
    sliders: Query<(&SliderValue, &SliderRange, &Children), With<Control>>,
    mut parts: Query<(&mut Node, &Part)>,
) {
    for (value, range, children) in &sliders {
        let span = (range.end() - range.start()).max(f32::EPSILON);
        let share = ((value.0 - range.start()) / span).clamp(0.0, 1.0);
        // The thumb travels the track minus its own width.
        let left = share * (132.0 - 16.0);
        for child in children.iter() {
            let Ok((mut node, part)) = parts.get_mut(child) else {
                continue;
            };
            match part {
                Part::Thumb if node.left != px(left) => node.left = px(left),
                Part::Travel if node.width != px(left + 8.0) => node.width = px(left + 8.0),
                _ => {}
            }
        }
    }
}

// --- Tables ------------------------------------------------------------------------

/// Column widths of a table in percent; the first column is the name.
pub fn table_header(
    world: &mut World,
    parent: Entity,
    columns: &[(&str, f32)],
    sorted: Option<(usize, bool)>,
) -> Entity {
    let row = world
        .spawn((
            ChildOf(parent),
            Swatch::Edge,
            Tone::new(Surface::Paper, Role::Caption),
            Node {
                width: percent(100),
                height: px(t::ROW_ONE_LINE),
                padding: UiRect::horizontal(px(6)),
                border: UiRect::bottom(px(1)),
                align_items: AlignItems::Center,
                flex_shrink: 0.0,
                ..default()
            },
            BorderColor::all(Color::NONE),
            BackgroundColor(Color::NONE),
        ))
        .id();
    for (index, (name, width)) in columns.iter().enumerate() {
        let cell = table_cell(world, row, *width, index == 0);
        label(
            world,
            cell,
            *name,
            t::DENSE,
            Tone::secondary(Surface::Paper),
        );
        if let Some((column, ascending)) = sorted
            && column == index
        {
            icon::icon(
                world,
                cell,
                if ascending {
                    Icon::SortUp
                } else {
                    Icon::SortDown
                },
                12.0,
                Tone::secondary(Surface::Paper),
            );
        }
    }
    row
}

/// A cell of a table row: numbers are right-aligned, the first column left.
pub fn table_cell(world: &mut World, row: Entity, width: f32, first: bool) -> Entity {
    world
        .spawn((
            ChildOf(row),
            Node {
                width: percent(width),
                column_gap: px(5),
                align_items: AlignItems::Center,
                justify_content: if first {
                    JustifyContent::FlexStart
                } else {
                    JustifyContent::FlexEnd
                },
                overflow: Overflow::clip_x(),
                ..default()
            },
        ))
        .id()
}

/// A group row, 24 high: a raised band with a caption.
pub fn table_group(world: &mut World, parent: Entity, name: &str) -> Entity {
    let row = world
        .spawn((
            ChildOf(parent),
            Swatch::Fill,
            Tone::new(Surface::Paper, Role::Raised),
            Node {
                width: percent(100),
                height: px(24),
                padding: UiRect::horizontal(px(6)),
                align_items: AlignItems::Center,
                flex_shrink: 0.0,
                ..default()
            },
            BackgroundColor(Color::NONE),
        ))
        .id();
    label(world, row, name, t::CAPTION, Tone::caption(Surface::Paper));
    row
}

/// A table row, 27 high, selectable. Fill it with [`table_cell`]s.
pub fn table_row(
    world: &mut World,
    parent: Entity,
    name: &str,
    reach: Reach,
    action: impl Bundle,
) -> Entity {
    let row = list_row(world, parent, name, false, reach, action);
    let mut node = world.get_mut::<Node>(row).expect("row");
    node.min_height = px(t::TABLE_ROW);
    node.height = px(t::TABLE_ROW);
    node.width = percent(100);
    node.column_gap = px(0);
    node.padding = UiRect::new(px(3), px(6), px(0), px(0));
    row
}

// --- Messages ----------------------------------------------------------------------

/// A toast: a 36 high pill on solid chrome with a shape, one sentence and an
/// optional small button. A failure has a critical edge and stays.
pub fn toast(
    world: &mut World,
    parent: Entity,
    status: Status,
    text: &str,
    picture: Option<Icon>,
) -> Entity {
    let entity = world
        .spawn((
            ChildOf(parent),
            Skin::ChromeSolid,
            Node {
                height: px(36),
                padding: UiRect::horizontal(px(14)),
                column_gap: px(8),
                border: UiRect::all(px(1)),
                border_radius: BorderRadius::all(px(18)),
                align_items: AlignItems::Center,
                flex_shrink: 0.0,
                ..default()
            },
            BackgroundColor(t::BARK_900),
            BorderColor::all(t::BARK_600),
            t::shadow(t::SHADOW_2),
        ))
        .id();
    if status == Status::Crit {
        world
            .entity_mut(entity)
            .insert((Swatch::Edge, Tone::status(Surface::Chrome, Status::Crit)))
            .remove::<Skin>();
    }
    icon::icon(
        world,
        entity,
        picture.unwrap_or(Icon::status(status)),
        16.0,
        Tone::status(Surface::Chrome, status),
    );
    label(world, entity, text, t::BODY, Tone::text(Surface::Chrome));
    entity
}

/// An inline confirm: replaces the row it belongs to with a question, one
/// line of consequence, a "keep" button and a danger button that repeats the
/// verb. Returns the card; add the two buttons with [`button`].
pub fn inline_confirm(
    world: &mut World,
    parent: Entity,
    question: &str,
    consequence: &str,
) -> (Entity, Entity) {
    let entity = card(world, parent, 10.0);
    label(world, entity, question, t::BODY, Tone::text(Surface::Paper));
    paragraph(
        world,
        entity,
        consequence,
        t::DENSE,
        Tone::secondary(Surface::Paper),
    );
    let buttons = world
        .spawn((
            ChildOf(entity),
            Node {
                column_gap: px(8),
                margin: UiRect::top(px(4)),
                justify_content: JustifyContent::FlexEnd,
                ..default()
            },
        ))
        .id();
    (entity, buttons)
}

/// A modal: a scrim over everything and a 440 wide panel with the deepest
/// shadow. Returns `(scrim, panel)`; the caller traps focus in the panel and
/// makes Esc the safe choice.
pub fn modal(world: &mut World, title: &str, body: &str) -> (Entity, Entity) {
    let scrim = world
        .spawn((
            Name::new(format!("Modal: {title}")),
            GlobalZIndex(t::layer::SCRIM),
            Hovered::default(),
            Node {
                position_type: PositionType::Absolute,
                left: px(0),
                right: px(0),
                top: px(0),
                bottom: px(0),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            BackgroundColor(t::SCRIM),
        ))
        .id();
    let panel = modal_panel(world, scrim, title, body);
    (scrim, panel)
}

/// The panel of a modal, for a caller that brings its own scrim.
pub fn modal_panel(world: &mut World, parent: Entity, title: &str, body: &str) -> Entity {
    let entity = world
        .spawn((
            ChildOf(parent),
            GlobalZIndex(t::layer::MODAL),
            panel(Node {
                width: px(440),
                max_width: percent(94),
                padding: UiRect::all(px(20)),
                row_gap: px(10),
                flex_direction: FlexDirection::Column,
                ..default()
            }),
        ))
        .id();
    world.entity_mut(entity).insert(t::shadow(t::SHADOW_3));
    label(world, entity, title, t::TITLE, Tone::text(Surface::Paper));
    if !body.is_empty() {
        paragraph(
            world,
            entity,
            body,
            t::BODY,
            Tone::secondary(Surface::Paper),
        );
    }
    entity
}

/// A caption line with the information icon: the reason under a disabled
/// control, in the same words as its tooltip.
pub fn reason(world: &mut World, parent: Entity, surface: Surface, text: &str) -> Entity {
    let entity = world
        .spawn((
            ChildOf(parent),
            Node {
                column_gap: px(5),
                align_items: AlignItems::Center,
                flex_shrink: 0.0,
                ..default()
            },
        ))
        .id();
    icon::icon(world, entity, Icon::Hint, 12.0, Tone::caption(surface));
    paragraph(world, entity, text, t::CAPTION, Tone::caption(surface));
    entity
}

// --- Paint -------------------------------------------------------------------------

fn skin(world: &mut World, entity: Entity, fill: Color, edge: Color) {
    if let Some(mut background) = world.get_mut::<BackgroundColor>(entity) {
        background.set_if_neq(BackgroundColor(fill));
    }
    if let Some(mut border) = world.get_mut::<BorderColor>(entity) {
        border.set_if_neq(BorderColor::all(edge));
    }
}

/// Resolve every static tone and skin from the theme.
pub(super) fn tones(world: &mut World) {
    let theme = world.get_resource::<Theme>().copied().unwrap_or_default();
    let skins: Vec<(Entity, Skin)> = world
        .query::<(Entity, &Skin)>()
        .iter(world)
        .map(|(entity, skin)| (entity, *skin))
        .collect();
    for (entity, kind) in skins {
        let paper = theme.roles(Surface::Paper);
        let (fill, edge) = match kind {
            Skin::Panel => (paper.surface, paper.frame),
            Skin::Chrome => (t::chrome_fill(), t::BARK_600),
            Skin::ChromeSolid => (t::BARK_900, t::BARK_600),
            Skin::Card => (paper.raised, paper.border_soft),
            Skin::Boxed => {
                let surface = world
                    .get::<Tone>(entity)
                    .map_or(Surface::Paper, |tone| tone.surface);
                let roles = theme.roles(surface);
                (roles.raised, roles.border_control)
            }
            Skin::Key(surface) => {
                // Inside a control the control paints the cap.
                if world.get::<Part>(entity).is_some() {
                    continue;
                }
                let roles = theme.roles(surface);
                (roles.key, roles.key_edge)
            }
        };
        skin(world, entity, fill, edge);
    }
    let toned: Vec<(Entity, Tone, Option<Swatch>, bool)> = world
        .query::<(Entity, &Tone, Option<&Swatch>, Has<Part>, Has<Skin>)>()
        .iter(world)
        .filter(|(.., skinned)| !skinned)
        .map(|(entity, tone, swatch, part, _)| (entity, *tone, swatch.copied(), part))
        .collect();
    for (entity, tone, swatch, part) in toned {
        let color = tone.color(&theme);
        match swatch {
            Some(Swatch::Fill) => {
                if let Some(mut background) = world.get_mut::<BackgroundColor>(entity) {
                    background.set_if_neq(BackgroundColor(color));
                }
            }
            Some(Swatch::Edge) => {
                if let Some(mut border) = world.get_mut::<BorderColor>(entity) {
                    border.set_if_neq(BorderColor::all(color));
                }
            }
            Some(Swatch::Both) => skin(world, entity, color, color),
            // A part of a control is coloured by its control.
            None if part => {}
            None => tint(world, entity, color),
        }
    }
}

/// Colour a text, an icon or a drawn glyph and the untoned nodes under it.
pub fn tint(world: &mut World, entity: Entity, color: Color) {
    if let Some(mut text) = world.get_mut::<TextColor>(entity) {
        text.set_if_neq(TextColor(color));
    }
    if world
        .get::<ImageNode>(entity)
        .is_some_and(|image| image.color != color)
        && let Some(mut image) = world.get_mut::<ImageNode>(entity)
    {
        image.color = color;
    }
    match world.get::<icon::GlyphPart>(entity).copied() {
        Some(icon::GlyphPart::Fill) => {
            if let Some(mut background) = world.get_mut::<BackgroundColor>(entity) {
                background.set_if_neq(BackgroundColor(color));
            }
        }
        Some(icon::GlyphPart::Ring) => {
            if let Some(mut border) = world.get_mut::<BorderColor>(entity) {
                border.set_if_neq(BorderColor::all(color));
            }
        }
        None => {}
    }
    let children = world
        .get::<Children>(entity)
        .map(|children| children.to_vec())
        .unwrap_or_default();
    for child in children {
        // A nested tone (the storm's caution mark) keeps its own colour.
        if world.get::<Tone>(child).is_none() {
            tint(world, child, color);
        }
    }
}

fn paint_parts(
    world: &mut World,
    entity: Entity,
    look: &Look,
    state: Interaction,
    filled: bool,
    depth: u8,
) {
    let children = world
        .get::<Children>(entity)
        .map(|children| children.to_vec())
        .unwrap_or_default();
    for child in children {
        if world.get::<Control>(child).is_some() {
            continue;
        }
        match world.get::<Part>(child).copied() {
            Some(Part::Label) => tint(world, child, look.ink),
            Some(Part::Secondary) => tint(world, child, look.ink_secondary),
            Some(Part::Travel) => {
                if let Some(mut background) = world.get_mut::<BackgroundColor>(child) {
                    background.set_if_neq(BackgroundColor(look.fill));
                }
            }
            Some(Part::Thumb) => {
                if let Some(mut background) = world.get_mut::<BackgroundColor>(child) {
                    background.set_if_neq(BackgroundColor(look.ink));
                }
                if let Some(mut border) = world.get_mut::<BorderColor>(child) {
                    border.set_if_neq(BorderColor::all(look.edge));
                }
                // The toggle's thumb sits at the right while on.
                if let Some(mut node) = world.get_mut::<Node>(child)
                    && node.position_type != PositionType::Absolute
                {
                    let margin = UiRect::left(px(if state.on { 16 } else { 0 }));
                    if node.margin != margin {
                        node.margin = margin;
                    }
                }
            }
            Some(Part::Key) => {
                // On a honey or moss fill: a tint of the text colour.
                let surface = match world.get::<Skin>(child) {
                    Some(Skin::Key(surface)) => *surface,
                    _ => Surface::Chrome,
                };
                let theme = world.get_resource::<Theme>().copied().unwrap_or_default();
                let roles = theme.roles(surface);
                let (fill, edge, ink) = if filled {
                    (
                        look.ink.with_alpha(0.14),
                        look.ink.with_alpha(0.45),
                        look.ink,
                    )
                } else if state.disabled {
                    (Color::NONE, roles.border_soft, look.ink)
                } else {
                    (roles.key, roles.key_edge, roles.key_ink)
                };
                skin(world, child, fill, edge);
                let texts = world
                    .get::<Children>(child)
                    .map(|children| children.to_vec())
                    .unwrap_or_default();
                for text in texts {
                    if let Some(mut color) = world.get_mut::<TextColor>(text) {
                        color.set_if_neq(TextColor(ink));
                    }
                }
            }
            None if depth > 0 => paint_parts(world, child, look, state, filled, depth - 1),
            None => {}
        }
    }
}

/// Paint every kit control from its state: fill, edge, focus ring and parts.
pub(crate) fn paint(world: &mut World) {
    let theme = world.get_resource::<Theme>().copied().unwrap_or_default();
    let focused = world
        .get_resource::<InputFocus>()
        .and_then(|focus| focus.get());
    let controls: Vec<(Entity, Control, Interaction, bool)> = world
        .query::<(
            Entity,
            &Control,
            Option<&Hovered>,
            Has<Pressed>,
            Has<InteractionDisabled>,
            Option<&Forced>,
        )>()
        .iter(world)
        .map(|(entity, control, hovered, pressed, disabled, forced)| {
            let forced = forced.copied().unwrap_or_default();
            (
                entity,
                *control,
                Interaction {
                    hovered: hovered.is_some_and(Hovered::get) || forced.hovered,
                    pressed: pressed || forced.pressed,
                    disabled,
                    on: control.on,
                    marked: control.marked,
                },
                focused == Some(entity) || forced.focused,
            )
        })
        .collect();
    for (entity, control, state, focus) in controls {
        let roles = theme.roles(control.surface);
        let look = look(control.kind, control.surface, state, roles);
        if control.kind != Kind::Slider {
            skin(world, entity, look.fill, look.edge);
        }
        let outline = if focus {
            Outline::new(px(2), px(2), roles.focus)
        } else if look.ring != Color::NONE {
            Outline::new(px(1), px(0), look.ring)
        } else {
            Outline::new(px(2), px(2), Color::NONE)
        };
        if world.get::<Outline>(entity).is_some_and(|current| {
            current.color != outline.color
                || current.width != outline.width
                || current.offset != outline.offset
        }) && let Some(mut current) = world.get_mut::<Outline>(entity)
        {
            *current = outline;
        }
        let filled = !state.disabled
            && (matches!(control.kind, Kind::Primary | Kind::Danger)
                || (state.on && look.fill != Color::NONE));
        paint_parts(world, entity, &look, state, filled, 2);
    }
}

/// Set a control's lit and marked states; a no-op when nothing changes.
pub fn set_state(world: &mut World, entity: Entity, on: bool, marked: bool) {
    if let Some(mut control) = world.get_mut::<Control>(entity)
        && (control.on != on || control.marked != marked)
    {
        control.on = on;
        control.marked = marked;
    }
}

/// The names of controls that show no word and carry no tooltip. A control
/// that is only a picture (or a picture and a key cap) must explain itself
/// on hover and on focus; tests of each panel expect this list to be empty.
pub fn unexplained_icon_controls(world: &mut World) -> Vec<String> {
    fn worded(world: &World, entity: Entity) -> bool {
        // A key cap is not a label.
        if matches!(world.get::<Skin>(entity), Some(Skin::Key(_))) {
            return false;
        }
        if world
            .get::<Text>(entity)
            .is_some_and(|text| !text.0.trim().is_empty())
        {
            return true;
        }
        world
            .get::<Children>(entity)
            .is_some_and(|children| children.iter().any(|child| worded(world, child)))
    }
    let controls: Vec<Entity> = world
        .query_filtered::<Entity, With<Control>>()
        .iter(world)
        .collect();
    let mut names: Vec<String> = controls
        .into_iter()
        .filter(|entity| {
            world
                .get::<crate::game_ui::tooltip::Tooltip>(*entity)
                .is_none()
                && !worded(world, *entity)
        })
        .map(|entity| {
            world
                .get::<Name>(entity)
                .map_or_else(|| format!("{entity:?}"), |name| name.to_string())
        })
        .collect();
    names.sort();
    names
}

pub fn set_enabled(world: &mut World, entity: Entity, enabled: bool) {
    let disabled = world.get::<InteractionDisabled>(entity).is_some();
    if enabled && disabled {
        world.entity_mut(entity).remove::<InteractionDisabled>();
    } else if !enabled && !disabled {
        world.entity_mut(entity).insert(InteractionDisabled);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game_ui::tokens::{CHROME, PARCHMENT, contrast};

    const KINDS: [Kind; 13] = [
        Kind::Primary,
        Kind::Secondary,
        Kind::Quiet,
        Kind::Danger,
        Kind::IconButton,
        Kind::DockTile,
        Kind::Segment,
        Kind::Tab,
        Kind::Row,
        Kind::Card,
        Kind::Chip,
        Kind::Toggle,
        Kind::Slider,
    ];

    fn states() -> Vec<Interaction> {
        let mut states = Vec::new();
        for bits in 0..32_u8 {
            states.push(Interaction {
                hovered: bits & 1 != 0,
                pressed: bits & 2 != 0,
                disabled: bits & 4 != 0,
                on: bits & 8 != 0,
                marked: bits & 16 != 0,
            });
        }
        states
    }

    #[test]
    fn enabled_labels_stay_legible_on_their_fill_in_every_state() {
        for (surface, roles, ground) in [
            (Surface::Chrome, &CHROME, t::BARK_900),
            (Surface::Paper, &PARCHMENT, t::PARCHMENT_100),
            (Surface::Paper, &t::DUSK, t::DUSK_100),
        ] {
            for kind in KINDS {
                if matches!(kind, Kind::Toggle | Kind::Slider) {
                    continue;
                }
                for state in states().into_iter().filter(|state| !state.disabled) {
                    let look = look(kind, surface, state, roles);
                    let under = if look.fill == Color::NONE {
                        ground
                    } else {
                        look.fill
                    };
                    let ratio = contrast(look.ink, under);
                    assert!(
                        ratio >= 4.5,
                        "{kind:?} on {surface:?} {state:?}: label {ratio:.2}"
                    );
                }
            }
        }
    }

    #[test]
    fn the_state_table_follows_the_spec() {
        let rest = Interaction::default();
        let hover = Interaction {
            hovered: true,
            ..rest
        };
        let press = Interaction {
            pressed: true,
            ..rest
        };
        let on = Interaction { on: true, ..rest };
        let off = Interaction {
            disabled: true,
            ..rest
        };
        let paper = |kind, state| look(kind, Surface::Paper, state, &PARCHMENT);
        let chrome = |kind, state| look(kind, Surface::Chrome, state, &CHROME);
        assert_eq!(paper(Kind::Primary, rest).fill, t::MOSS_600);
        assert_eq!(paper(Kind::Primary, hover).fill, t::MOSS_700);
        assert_eq!(paper(Kind::Primary, press).fill, t::MOSS_800);
        assert_eq!(paper(Kind::Primary, rest).ink, t::CREAM_50);
        assert_eq!(paper(Kind::Secondary, rest).fill, t::PARCHMENT_200);
        assert_eq!(paper(Kind::Secondary, rest).edge, t::BORDER_CONTROL);
        assert_eq!(paper(Kind::Secondary, hover).fill, t::PARCHMENT_300);
        assert_eq!(paper(Kind::Secondary, press).fill, t::BORDER_SOFT);
        assert_eq!(chrome(Kind::Secondary, rest).fill, t::BARK_800);
        assert_eq!(chrome(Kind::Secondary, hover).fill, t::BARK_700);
        assert_eq!(chrome(Kind::Secondary, press).fill, t::BARK_900);
        assert_eq!(paper(Kind::Quiet, rest).fill, Color::NONE);
        assert_eq!(paper(Kind::Quiet, rest).ink, t::INK_700);
        assert_eq!(paper(Kind::Quiet, hover).fill, t::PARCHMENT_300);
        assert_eq!(paper(Kind::Danger, rest).fill, t::CRIT_INK);
        assert_ne!(paper(Kind::Danger, hover).fill, t::CRIT_INK);
        // Disabled keeps its place with quiet colours on both surfaces.
        assert_eq!(paper(Kind::Primary, off).fill, t::PARCHMENT_200);
        assert_eq!(paper(Kind::Primary, off).edge, t::BORDER_SOFT);
        assert_eq!(paper(Kind::Primary, off).ink, t::BORDER_CONTROL);
        assert_eq!(chrome(Kind::Secondary, off).fill, t::BARK_800);
        assert_eq!(chrome(Kind::Secondary, off).ink, t::BARK_600);
        // The lit segment: honey with dark ink on chrome, bark with cream on paper.
        assert_eq!(chrome(Kind::Segment, on).fill, t::HONEY_500);
        assert_eq!(chrome(Kind::Segment, on).ink, t::INK_900);
        assert_eq!(paper(Kind::Segment, on).fill, t::BARK_700);
        assert_eq!(paper(Kind::Segment, on).ink, t::CREAM_50);
        assert_eq!(chrome(Kind::DockTile, on).fill, t::HONEY_500);
        assert_eq!(chrome(Kind::DockTile, hover).fill, t::BARK_700);
        assert_eq!(chrome(Kind::DockTile, off).ink, t::BARK_600);
        assert_eq!(chrome(Kind::IconButton, on).fill, t::HONEY_500);
        // Tabs, rows and cards mark selection with honey edges.
        assert_eq!(paper(Kind::Tab, on).edge, t::HONEY_700);
        assert_eq!(paper(Kind::Tab, on).fill, t::PARCHMENT_200);
        assert_eq!(paper(Kind::Tab, rest).ink, t::INK_700);
        assert_eq!(paper(Kind::Row, on).fill, t::HONEY_200);
        assert_eq!(paper(Kind::Row, on).edge, t::HONEY_700);
        assert_eq!(paper(Kind::Row, hover).fill, t::PARCHMENT_200);
        assert_eq!(paper(Kind::Card, on).ring, t::HONEY_700);
        assert_eq!(paper(Kind::Card, hover).fill, t::PARCHMENT_300);
        let unavailable = Interaction {
            marked: true,
            ..rest
        };
        assert_eq!(paper(Kind::Card, unavailable).fill, Color::NONE);
        // A total in use while placing gains a honey edge.
        assert_eq!(chrome(Kind::Chip, unavailable).edge, t::HONEY_500);
        assert_eq!(chrome(Kind::Chip, hover).fill, t::BARK_800);
        assert_eq!(paper(Kind::Chip, rest).fill, t::PARCHMENT_200);
        // The toggle shows its state by fill and thumb.
        assert_eq!(paper(Kind::Toggle, on).fill, t::MOSS_600);
        assert_eq!(paper(Kind::Toggle, rest).fill, t::PARCHMENT_300);
    }

    #[test]
    fn widgets_build_with_their_sizes_and_join_the_keyboard_path() {
        let mut world = World::new();
        let root = world.spawn(Node::default()).id();
        let primary = button(
            &mut world,
            root,
            ButtonSpec::new("Save game", Kind::Primary, Surface::Paper).key("Ctrl+S"),
            (),
        );
        assert_eq!(world.get::<Node>(primary).unwrap().height, px(36.0));
        assert_eq!(world.get::<TabIndex>(primary), Some(&TabIndex(0)));
        assert!(world.get::<WidgetButton>(primary).is_some());
        // The label never carries the key: it is a key cap part.
        let children = world.get::<Children>(primary).unwrap().to_vec();
        assert_eq!(world.get::<Text>(children[0]).unwrap().0, "Save game");
        assert_eq!(world.get::<Part>(children[1]), Some(&Part::Key));
        let tile = dock_tile(
            &mut world,
            root,
            "Village",
            Icon::VillagePage,
            Some("F1"),
            (),
        );
        let node = world.get::<Node>(tile.entity).unwrap();
        assert_eq!((node.width, node.height), (px(64.0), px(44.0)));
        // Dock tiles are reached with Tab but never focused by a click.
        assert!(world.get::<KeyFocus>(tile.entity).is_some());
        assert!(world.get::<TabIndex>(tile.entity).is_none());
        let icon = icon_button(
            &mut world,
            root,
            "Menu",
            Icon::Menu,
            Surface::Chrome,
            Reach::Ring,
            (),
        );
        let node = world.get::<Node>(icon).unwrap();
        assert_eq!((node.width, node.height), (px(32.0), px(32.0)));
        assert_eq!(
            world.get::<Name>(icon).map(Name::as_str),
            Some("Game: Menu"),
            "an icon-only control is still named"
        );
        let cap = keycap(&mut world, root, Surface::Paper, "Esc");
        assert_eq!(world.get::<Node>(cap).unwrap().height, px(18.0));
        let chip = stat_chip(
            &mut world,
            root,
            "Planks",
            Icon::Planks,
            "11",
            None,
            Surface::Chrome,
            Reach::Ring,
            (),
        );
        assert_eq!(world.get::<Node>(chip.entity).unwrap().height, px(28.0));
        set_mark(
            &mut world,
            chip.mark,
            Some((Icon::Low, Status::Warn)),
            Surface::Chrome,
        );
        assert_eq!(world.get::<Children>(chip.mark).unwrap().len(), 1);
        set_mark(&mut world, chip.mark, None, Surface::Chrome);
        assert!(
            world
                .get::<Children>(chip.mark)
                .is_none_or(|c| c.is_empty())
        );
        let bar = progress_bar(&mut world, root, Surface::Paper, 0.25, Some("200 / 800"));
        assert_eq!(world.get::<Node>(bar.fill).unwrap().width, percent(25.0));
        set_bar(&mut world, bar, 2.0, Some("800 / 800"));
        assert_eq!(world.get::<Node>(bar.fill).unwrap().width, percent(100.0));
    }

    #[test]
    fn paint_applies_the_look_and_shows_focus_as_an_outline() {
        let mut world = World::new();
        world.init_resource::<InputFocus>();
        let root = world.spawn(Node::default()).id();
        let entity = button(
            &mut world,
            root,
            ButtonSpec::new("Resume", Kind::Primary, Surface::Paper),
            (),
        );
        paint(&mut world);
        assert_eq!(world.get::<BackgroundColor>(entity).unwrap().0, t::MOSS_600);
        assert_eq!(world.get::<Outline>(entity).unwrap().color, Color::NONE);
        let text = world.get::<Children>(entity).unwrap()[0];
        assert_eq!(world.get::<TextColor>(text).unwrap().0, t::CREAM_50);
        world
            .resource_mut::<InputFocus>()
            .set(entity, bevy::input_focus::FocusCause::Navigated);
        world.entity_mut(entity).insert(Forced {
            hovered: true,
            ..default()
        });
        paint(&mut world);
        assert_eq!(world.get::<BackgroundColor>(entity).unwrap().0, t::MOSS_700);
        assert_eq!(world.get::<Outline>(entity).unwrap().color, t::SEA_600);
        // Disabled: quiet colours, and it stays a control.
        world.entity_mut(entity).insert(InteractionDisabled);
        paint(&mut world);
        assert_eq!(
            world.get::<BackgroundColor>(entity).unwrap().0,
            t::PARCHMENT_200
        );
        assert_eq!(world.get::<TextColor>(text).unwrap().0, t::BORDER_CONTROL);
        // Dusk swaps the table, not the widget.
        world.insert_resource(Theme {
            style: t::PanelStyle::Dusk,
        });
        world.entity_mut(entity).remove::<InteractionDisabled>();
        let row = list_row(&mut world, root, "Ada", true, Reach::Ring, ());
        set_state(&mut world, row, true, false);
        paint(&mut world);
        assert_eq!(
            world.get::<BackgroundColor>(row).unwrap().0,
            t::DUSK_SELECTED
        );
        assert_eq!(world.get::<Outline>(entity).unwrap().color, t::HONEY_500);
    }
}
