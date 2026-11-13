//! A developer gallery of the widget kit: every widget and icon in every
//! state, for screenshot verification. View only; opened by the host's
//! `--ui-gallery` flag and never from the player menu.
use super::icon::{self, Drawn, Icon, Source};
use super::tokens::{self as t, PanelStyle, Status, Surface, Theme};
use super::tooltip::{self, Tooltip};
use super::widgets::{self as w, ButtonSpec, Forced, Kind, Reach, Role, Size, Swatch, Tone};
use bevy::prelude::*;
use bevy::ui::InteractionDisabled;
use bevy::ui_widgets::{Activate, slider_self_update};

/// The gallery's pages.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Page {
    Tokens,
    #[default]
    Controls,
    Data,
    Messages,
    Icons,
}

impl Page {
    pub const ALL: [Self; 5] = [
        Self::Tokens,
        Self::Controls,
        Self::Data,
        Self::Messages,
        Self::Icons,
    ];
    pub const fn label(self) -> &'static str {
        match self {
            Self::Tokens => "Tokens",
            Self::Controls => "Controls",
            Self::Data => "Data",
            Self::Messages => "Messages",
            Self::Icons => "Icons",
        }
    }
    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|page| page.label().eq_ignore_ascii_case(value))
    }
}

#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Gallery {
    pub page: Page,
    shown: Option<(Page, PanelStyle)>,
}

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
enum Action {
    Page(Page),
    Style(PanelStyle),
}
#[derive(Component)]
struct GalleryRoot;
#[derive(Component)]
struct GalleryBody;

/// Show the gallery over everything, starting on `page`.
pub fn install(app: &mut App, page: Page) {
    super::install(app);
    app.insert_resource(Gallery { page, shown: None })
        .add_observer(activate)
        .add_systems(Startup, spawn)
        .add_systems(Update, refresh);
}

fn activate(
    event: On<Activate>,
    actions: Query<&Action>,
    mut gallery: ResMut<Gallery>,
    mut theme: ResMut<Theme>,
) {
    match actions.get(event.entity) {
        Ok(Action::Page(page)) => gallery.page = *page,
        Ok(Action::Style(style)) => theme.style = *style,
        Err(_) => {}
    }
}

fn spawn(world: &mut World) {
    let root = world
        .spawn((
            Name::new("Interface gallery"),
            GalleryRoot,
            crate::FrontendUiRoot,
            // Above the host's title and pause screens.
            GlobalZIndex(200),
            w::Skin::Panel,
            Node {
                position_type: PositionType::Absolute,
                left: px(0),
                right: px(0),
                top: px(0),
                bottom: px(0),
                flex_direction: FlexDirection::Column,
                ..default()
            },
            BackgroundColor(t::PARCHMENT_100),
            BorderColor::all(Color::NONE),
            bevy::picking::hover::Hovered::default(),
        ))
        .id();
    let bar = world
        .spawn((
            ChildOf(root),
            w::chrome(
                Node {
                    height: px(t::TOP_BAR),
                    min_height: px(t::TOP_BAR),
                    padding: UiRect::horizontal(px(8)),
                    column_gap: px(8),
                    align_items: AlignItems::Center,
                    border: UiRect::bottom(px(1)),
                    ..default()
                },
                0.0,
            ),
        ))
        .id();
    w::label(
        world,
        bar,
        "Interface gallery",
        t::SUBHEADING,
        Tone::text(Surface::Chrome),
    );
    w::divider(world, bar, Surface::Chrome, true);
    let pages = w::segmented(world, bar, Surface::Chrome);
    for page in Page::ALL {
        w::segment(
            world,
            pages,
            page.label(),
            None,
            Some(page.label()),
            None,
            Reach::Ring,
            Action::Page(page),
        );
    }
    world.spawn((
        ChildOf(bar),
        Node {
            flex_grow: 1.0,
            ..default()
        },
    ));
    w::label(
        world,
        bar,
        "Panel style",
        t::DENSE,
        Tone::secondary(Surface::Chrome),
    );
    let styles = w::segmented(world, bar, Surface::Chrome);
    for (name, style) in [
        ("Parchment", PanelStyle::Parchment),
        ("Dusk", PanelStyle::Dusk),
    ] {
        w::segment(
            world,
            styles,
            name,
            None,
            Some(name),
            None,
            Reach::Ring,
            Action::Style(style),
        );
    }
    world.spawn((
        ChildOf(root),
        GalleryBody,
        Node {
            flex_grow: 1.0,
            min_height: px(0),
            padding: UiRect::all(px(12)),
            column_gap: px(12),
            row_gap: px(12),
            flex_wrap: FlexWrap::Wrap,
            align_content: AlignContent::FlexStart,
            overflow: Overflow::clip(),
            ..default()
        },
    ));
}

fn refresh(world: &mut World) {
    let gallery = *world.resource::<Gallery>();
    let style = world.resource::<Theme>().style;
    let actions: Vec<(Entity, Action)> = world
        .query::<(Entity, &Action)>()
        .iter(world)
        .map(|(entity, action)| (entity, *action))
        .collect();
    for (entity, action) in actions {
        let on = match action {
            Action::Page(page) => page == gallery.page,
            Action::Style(wanted) => wanted == style,
        };
        w::set_state(world, entity, on, false);
    }
    if gallery.shown == Some((gallery.page, style)) {
        return;
    }
    let Some(body) = world
        .query_filtered::<Entity, With<GalleryBody>>()
        .iter(world)
        .next()
    else {
        return;
    };
    world.entity_mut(body).despawn_children();
    match gallery.page {
        Page::Tokens => tokens(world, body),
        Page::Controls => controls(world, body),
        Page::Data => data(world, body),
        Page::Messages => messages(world, body),
        Page::Icons => icons(world, body),
    }
    world.resource_mut::<Gallery>().shown = Some((gallery.page, style));
}

/// A titled block: parchment, or chrome for the controls that live on bars.
fn block(world: &mut World, parent: Entity, title: &str, surface: Surface, width: f32) -> Entity {
    let entity = world
        .spawn((
            ChildOf(parent),
            Node {
                width: percent(width),
                flex_direction: FlexDirection::Column,
                row_gap: px(8),
                padding: UiRect::all(px(12)),
                border: UiRect::all(px(1)),
                border_radius: BorderRadius::all(px(t::RADIUS_LG)),
                flex_shrink: 1.0,
                ..default()
            },
            BackgroundColor(Color::NONE),
            BorderColor::all(Color::NONE),
        ))
        .id();
    if surface == Surface::Chrome {
        world.entity_mut(entity).insert(w::Skin::ChromeSolid);
    } else {
        world
            .entity_mut(entity)
            .insert((Swatch::Edge, Tone::new(Surface::Paper, Role::Edge)));
    }
    w::label(world, entity, title, t::SUBHEADING, Tone::text(surface));
    entity
}

fn line(world: &mut World, parent: Entity) -> Entity {
    world
        .spawn((
            ChildOf(parent),
            Node {
                align_items: AlignItems::Center,
                column_gap: px(8),
                row_gap: px(6),
                flex_wrap: FlexWrap::Wrap,
                ..default()
            },
        ))
        .id()
}

fn caption(world: &mut World, parent: Entity, surface: Surface, text: &str) {
    w::label(world, parent, text, t::CAPTION, Tone::caption(surface));
}

fn swatch(world: &mut World, parent: Entity, color: Color, name: &str) {
    let row = line(world, parent);
    world.spawn((
        ChildOf(row),
        Node {
            width: px(28),
            height: px(16),
            border: UiRect::all(px(1)),
            border_radius: BorderRadius::all(px(3)),
            ..default()
        },
        BackgroundColor(color),
        BorderColor::all(Color::srgba(0.0, 0.0, 0.0, 0.35)),
    ));
    caption(world, row, Surface::Paper, name);
}

fn tokens(world: &mut World, body: Entity) {
    let surfaces = block(world, body, "Surfaces and text", Surface::Paper, 32.0);
    for (color, name) in [
        (t::BARK_900, "bark-900 #2B2018 · chrome"),
        (t::BARK_800, "bark-800 #3A2C21 · control on chrome"),
        (t::BARK_700, "bark-700 #4C3B2D · hover, panel frame"),
        (t::BARK_600, "bark-600 #6B5443 · chrome border"),
        (t::PARCHMENT_100, "parchment-100 #F5ECD7 · panel"),
        (t::PARCHMENT_200, "parchment-200 #ECDFC1 · card, track"),
        (t::PARCHMENT_300, "parchment-300 #DFCEA8 · hover, key cap"),
        (t::BORDER_SOFT, "border-soft #C8B48A · dividers"),
        (t::BORDER_CONTROL, "border-control #8C744D · control edges"),
        (t::INK_900, "ink-900 #2B1F16 · text"),
        (t::INK_700, "ink-700 #55432F · secondary text"),
        (t::INK_500, "ink-500 #6A5642 · captions"),
        (t::CREAM_50, "cream-50 #FBF5E6 · text on chrome"),
        (t::CREAM_300, "cream-300 #D8C8AA · secondary on chrome"),
    ] {
        swatch(world, surfaces, color, name);
    }
    let accent = block(
        world,
        body,
        "Accent and status (colour + shape)",
        Surface::Paper,
        32.0,
    );
    for (color, name) in [
        (t::HONEY_500, "honey-500 #E8B04B · active, focus on chrome"),
        (t::HONEY_200, "honey-200 #F8E3A8 · selected row or card"),
        (t::HONEY_700, "honey-700 #8A5A0B · selected edge, tab line"),
        (t::MOSS_600, "moss-600 #3F6B34 · primary action"),
        (t::SEA_600, "sea-600 #1F6A8C · focus on parchment, links"),
    ] {
        swatch(world, accent, color, name);
    }
    for surface in [Surface::Paper, Surface::Chrome] {
        let holder = if surface == Surface::Chrome {
            let dark = world
                .spawn((
                    ChildOf(accent),
                    w::Skin::ChromeSolid,
                    Node {
                        padding: UiRect::axes(px(10), px(8)),
                        border: UiRect::all(px(1)),
                        border_radius: BorderRadius::all(px(t::RADIUS_MD)),
                        ..default()
                    },
                    BackgroundColor(t::BARK_900),
                    BorderColor::all(t::BARK_600),
                ))
                .id();
            line(world, dark)
        } else {
            line(world, accent)
        };
        for (status, word) in [
            (Status::Good, "fine"),
            (Status::Warn, "low"),
            (Status::Crit, "danger"),
            (Status::Info, "note"),
            (Status::Idle, "unchanged"),
        ] {
            icon::icon(
                world,
                holder,
                Icon::status(status),
                16.0,
                Tone::status(surface, status),
            );
            w::label(world, holder, word, t::DENSE, Tone::text(surface));
        }
    }
    let row = line(world, accent);
    for (picture, status, word) in [
        (Icon::TrendUp, Status::Good, "up 7"),
        (Icon::TrendDown, Status::Crit, "down 2"),
        (Icon::Unchanged, Status::Idle, "steady"),
        (Icon::Done, Status::Good, "done"),
        (Icon::Cannot, Status::Crit, "cannot"),
    ] {
        icon::icon(
            world,
            row,
            picture,
            14.0,
            Tone::status(Surface::Paper, status),
        );
        w::label(world, row, word, t::DENSE, Tone::text(Surface::Paper));
    }
    let kinds = block(world, body, "Type (Barlow Regular)", Surface::Paper, 32.0);
    for (step, name) in [
        (t::TITLE, "Title 28 / 34"),
        (t::HEADING, "Heading 20 / 26"),
        (t::SUBHEADING, "Subheading 16 / 22"),
        (t::BODY, "Body 14 / 20"),
        (t::DENSE, "Dense 13 / 18"),
        (t::CAPTION, "Caption 12 / 16 (smallest size)"),
    ] {
        w::label(world, kinds, name, step, Tone::text(Surface::Paper));
    }
    w::divider(world, kinds, Surface::Paper, false);
    caption(
        world,
        kinds,
        Surface::Paper,
        "Radius 4 · 6 · 10 and elevation 1 · 2 · 3",
    );
    let row = line(world, kinds);
    world.entity_mut(row).insert(Node {
        column_gap: px(16),
        padding: UiRect::all(px(8)),
        ..default()
    });
    for (radius, level) in [
        (t::RADIUS_SM, t::SHADOW_1),
        (t::RADIUS_MD, t::SHADOW_2),
        (t::RADIUS_LG, t::SHADOW_3),
    ] {
        world.spawn((
            ChildOf(row),
            w::Skin::Card,
            Node {
                width: px(64),
                height: px(40),
                border: UiRect::all(px(1)),
                border_radius: BorderRadius::all(px(radius)),
                ..default()
            },
            BackgroundColor(Color::NONE),
            BorderColor::all(Color::NONE),
            t::shadow(level),
        ));
    }
    caption(
        world,
        kinds,
        Surface::Paper,
        "Space 2 · 4 · 6 · 8 · 12 · 16 · 24 · 32",
    );
    let row = line(world, kinds);
    for space in t::SPACE {
        world.spawn((
            ChildOf(row),
            Swatch::Fill,
            Tone::new(Surface::Paper, Role::Accent),
            Node {
                width: px(space),
                height: px(16),
                ..default()
            },
            BackgroundColor(Color::NONE),
        ));
    }
}

fn forced(world: &mut World, entity: Entity, state: &str) {
    let forced = match state {
        "Hover" => Forced {
            hovered: true,
            ..default()
        },
        "Pressed" => Forced {
            pressed: true,
            ..default()
        },
        "Focus" => Forced {
            focused: true,
            ..default()
        },
        _ => return,
    };
    world.entity_mut(entity).insert(forced);
}

fn controls(world: &mut World, body: Entity) {
    for surface in [Surface::Paper, Surface::Chrome] {
        let title = if surface == Surface::Paper {
            "Buttons on parchment"
        } else {
            "Buttons on chrome"
        };
        let buttons = block(world, body, title, surface, 32.0);
        for kind in [Kind::Primary, Kind::Secondary] {
            let row = line(world, buttons);
            for state in ["Default", "Hover", "Pressed", "Focus"] {
                let entity = w::button(
                    world,
                    row,
                    ButtonSpec::new(state, kind, surface).reach(Reach::Ring),
                    (),
                );
                forced(world, entity, state);
            }
        }
        let row = line(world, buttons);
        let disabled = w::button(
            world,
            row,
            ButtonSpec::new("Build this cottage", Kind::Primary, surface).reach(Reach::Ring),
            InteractionDisabled,
        );
        world.entity_mut(disabled).insert(
            Tooltip::new("Build this cottage", "Needs 8 planks. You have 3.").icon(Icon::Cottage),
        );
        w::button(
            world,
            row,
            ButtonSpec::new("Quiet", Kind::Quiet, surface).reach(Reach::Ring),
            (),
        );
        w::button(
            world,
            row,
            ButtonSpec::new("Abandon it", Kind::Danger, surface).reach(Reach::Ring),
            (),
        );
        w::reason(world, buttons, surface, "Needs 8 planks. You have 3.");
        let row = line(world, buttons);
        for (name, size) in [
            ("Small 28", Size::Small),
            ("Medium 36", Size::Medium),
            ("Large 44", Size::Large),
        ] {
            w::button(
                world,
                row,
                ButtonSpec::new(name, Kind::Secondary, surface)
                    .size(size)
                    .reach(Reach::Ring),
                (),
            );
        }
        let row = line(world, buttons);
        w::button(
            world,
            row,
            ButtonSpec::new("Resume", Kind::Primary, surface)
                .key("Esc")
                .reach(Reach::Ring),
            (),
        );
        w::button(
            world,
            row,
            ButtonSpec::new("Save game", Kind::Secondary, surface)
                .icon(Icon::Done)
                .key("Ctrl+S")
                .reach(Reach::Ring),
            (),
        );
        caption(world, buttons, surface, "Key caps and icon buttons");
        let row = line(world, buttons);
        for key in [
            "E",
            "Esc",
            "Ctrl+S",
            "Space",
            "Click",
            "Right-click",
            "Wheel",
        ] {
            w::keycap(world, row, surface, key);
        }
        let row = line(world, buttons);
        for (name, picture, state, on) in [
            ("Menu", Icon::Menu, "Default", false),
            ("Follow", Icon::Follow, "Hover", false),
            ("Close", Icon::Close, "Focus", false),
            ("Following", Icon::Follow, "Default", true),
        ] {
            let entity = w::icon_button(world, row, name, picture, surface, Reach::Ring, ());
            forced(world, entity, state);
            w::set_state(world, entity, on, false);
        }
        let off = w::icon_button(
            world,
            row,
            "Locked",
            Icon::Locked,
            surface,
            Reach::Ring,
            InteractionDisabled,
        );
        let _ = off;
        caption(world, buttons, surface, "Segmented, toggle, slider");
        let row = line(world, buttons);
        let group = w::segmented(world, row, surface);
        for (index, name) in ["All", "Harbour", "Alderwood"].into_iter().enumerate() {
            let entity = w::segment(world, group, name, None, Some(name), None, Reach::Ring, ());
            w::set_state(world, entity, index == 1, false);
            if index == 2 {
                forced(world, entity, "Hover");
            }
        }
        let row = line(world, buttons);
        for (name, on, state) in [
            ("Edge pan", true, "Default"),
            ("Mute", false, "Default"),
            ("Show tips", true, "Focus"),
        ] {
            let entity = w::toggle(world, row, name, surface, on, Reach::Ring, ());
            forced(world, entity, state);
            w::label(world, row, name, t::BODY, Tone::text(surface));
        }
        let off = w::toggle(
            world,
            row,
            "Unavailable",
            surface,
            false,
            Reach::Ring,
            InteractionDisabled,
        );
        let _ = off;
        let row = line(world, buttons);
        w::label(world, row, "Interface size", t::BODY, Tone::text(surface));
        let slider = w::slider(
            world,
            row,
            "Interface size",
            surface,
            (80.0, 140.0),
            10.0,
            100.0,
            "100%",
            Reach::Ring,
            (),
        );
        world.entity_mut(slider.entity).observe(slider_self_update);
    }
    let dock = block(world, body, "Bars and docks", Surface::Chrome, 32.0);
    let row = line(world, dock);
    let modes = w::segmented(world, row, Surface::Chrome);
    for (index, (name, picture, key)) in [
        ("Live", Icon::Live, "1"),
        ("Oversee", Icon::Oversee, "2"),
        ("Shape", Icon::Shape, "3"),
    ]
    .into_iter()
    .enumerate()
    {
        let entity = w::segment(
            world,
            modes,
            name,
            Some(picture),
            Some(name),
            Some(key),
            Reach::Ring,
            (),
        );
        w::set_state(world, entity, index == 1, false);
    }
    let row = line(world, dock);
    let speed = w::segmented(world, row, Surface::Chrome);
    for (index, name) in ["Pause", "1×", "3×", "10×"].into_iter().enumerate() {
        let entity = if index == 0 {
            w::segment(
                world,
                speed,
                name,
                Some(Icon::Pause),
                None,
                None,
                Reach::Ring,
                (),
            )
        } else {
            w::segment(world, speed, name, None, Some(name), None, Reach::Ring, ())
        };
        w::set_state(world, entity, index == 1, false);
    }
    w::keycap(world, row, Surface::Chrome, "Space");
    w::keycap(world, row, Surface::Chrome, "-");
    w::keycap(world, row, Surface::Chrome, "=");
    let row = line(world, dock);
    for (name, state, on, enabled) in [
        ("Default", "Default", false, true),
        ("Hover", "Hover", false, true),
        ("Active", "Default", true, true),
        ("Focus", "Focus", false, true),
        ("Disabled", "Default", false, false),
    ] {
        let tile = w::dock_tile(world, row, name, Icon::Homes, Some("F1"), ());
        forced(world, tile.entity, state);
        w::set_state(world, tile.entity, on, false);
        w::set_enabled(world, tile.entity, enabled);
        if name == "Hover" {
            w::set_text(world, tile.badge_text, "3");
            w::show(world, tile.badge, true);
        }
    }
    let row = line(world, dock);
    for (name, picture, value, unit, mark, in_use) in [
        (
            "Residents",
            Icon::Residents,
            "42",
            Some("/ 84"),
            None,
            false,
        ),
        ("Morale", Icon::Morale, "97%", None, None, false),
        (
            "Food",
            Icon::Food,
            "114",
            None,
            Some((Icon::TrendUp, Status::Good)),
            false,
        ),
        (
            "Planks",
            Icon::Planks,
            "11",
            None,
            Some((Icon::Low, Status::Warn)),
            true,
        ),
        ("Civic fund", Icon::Coins, "0", Some("fund"), None, false),
    ] {
        let chip = w::stat_chip(
            world,
            row,
            name,
            picture,
            value,
            unit,
            Surface::Chrome,
            Reach::Ring,
            (),
        );
        w::set_mark(world, chip.mark, mark, Surface::Chrome);
        w::set_state(world, chip.entity, false, in_use);
    }
    for (name, picture, fraction, status) in [
        ("Water", Icon::Water, 0.34, Status::Warn),
        ("Food", Icon::Food, 0.12, Status::Crit),
        ("Rest", Icon::Rest, 0.86, Status::Good),
    ] {
        w::need_bar(
            world,
            dock,
            Surface::Chrome,
            picture,
            name,
            fraction,
            status,
        );
    }
    w::progress_bar(world, dock, Surface::Chrome, 0.4, Some("2 of 5"));
}

fn data(world: &mut World, body: Entity) {
    let rows = block(world, body, "Rows, tabs, bars", Surface::Paper, 32.0);
    let strip = w::tabs(world, rows);
    for (index, (name, picture)) in [
        ("Overview", Icon::Resident),
        ("Day", Icon::Time),
        ("Why", Icon::Goal),
    ]
    .into_iter()
    .enumerate()
    {
        let entity = w::tab(world, strip, name, Some(picture), Reach::Ring, ());
        w::set_state(world, entity, index == 0, false);
        if index == 2 {
            forced(world, entity, "Focus");
        }
    }
    for (name, detail, status, state, on) in [
        (
            "Alden · miller · Stone Ridge",
            "milling flour",
            Status::Good,
            "Default",
            false,
        ),
        (
            "Bram · miner · Harbour",
            "in need: food",
            Status::Warn,
            "Default",
            true,
        ),
        (
            "Cedar · hunter · Stone Ridge",
            "in danger: cold",
            Status::Crit,
            "Hover",
            false,
        ),
        (
            "Dara · tailor · Alderwood",
            "sewing a coat",
            Status::Good,
            "Focus",
            false,
        ),
    ] {
        let row = w::list_row(world, rows, name, false, Reach::Ring, ());
        icon::icon(
            world,
            row,
            Icon::status(status),
            16.0,
            Tone::status(Surface::Paper, status),
        );
        let text = w::label(world, row, name, t::DENSE, Tone::text(Surface::Paper));
        world.entity_mut(text).insert((
            w::Part::Label,
            Node {
                flex_grow: 1.0,
                ..default()
            },
        ));
        w::label(
            world,
            row,
            detail,
            t::DENSE,
            Tone::secondary(Surface::Paper),
        );
        forced(world, row, state);
        w::set_state(world, row, on, false);
    }
    let two = w::list_row(world, rows, "Ada", true, Reach::Ring, ());
    icon::icon(
        world,
        two,
        Icon::Fine,
        16.0,
        Tone::status(Surface::Paper, Status::Good),
    );
    let column = world
        .spawn((
            ChildOf(two),
            Node {
                flex_direction: FlexDirection::Column,
                flex_grow: 1.0,
                ..default()
            },
        ))
        .id();
    w::label(
        world,
        column,
        "Ada · farmer · Harbour",
        t::DENSE,
        Tone::text(Surface::Paper),
    );
    w::label(
        world,
        column,
        "Gathering fiber · mood 90% · 12 coins",
        t::CAPTION,
        Tone::secondary(Surface::Paper),
    );
    icon::icon(world, two, Icon::GoTo, 12.0, Tone::caption(Surface::Paper));
    let row = line(world, rows);
    w::label(world, row, "Progress", t::DENSE, Tone::text(Surface::Paper));
    let bar = w::progress_bar(world, row, Surface::Paper, 0.33, Some("200 / 600"));
    world.entity_mut(bar.entity).insert(Node {
        flex_grow: 1.0,
        align_items: AlignItems::Center,
        column_gap: px(8),
        ..default()
    });
    for (name, picture, fraction, status) in [
        ("Food", Icon::Food, 0.96, Status::Good),
        ("Warmth", Icon::Warmth, 0.41, Status::Warn),
        ("Water", Icon::Water, 0.08, Status::Crit),
    ] {
        w::need_bar(world, rows, Surface::Paper, picture, name, fraction, status);
    }

    let table = block(world, body, "Table", Surface::Paper, 32.0);
    let columns = [
        ("Good", 40.0),
        ("Stock", 16.0),
        ("Price", 14.0),
        ("Since yesterday", 30.0),
    ];
    w::table_header(world, table, &columns, Some((1, false)));
    w::table_group(world, table, "Food and water");
    for (name, picture, stock, price, trend, word, low, on, state) in [
        (
            "Bread",
            Icon::Bread,
            "102",
            "3",
            Icon::Unchanged,
            "steady",
            false,
            false,
            "Default",
        ),
        (
            "Clean water",
            Icon::CleanWater,
            "104",
            "1",
            Icon::TrendUp,
            "up 9",
            false,
            false,
            "Hover",
        ),
        (
            "Fish",
            Icon::Fish,
            "38",
            "2",
            Icon::TrendDown,
            "down 2",
            false,
            false,
            "Default",
        ),
    ] {
        table_line(
            world,
            table,
            &columns,
            (name, picture, stock, price, trend, word, low, on, state),
        );
    }
    w::table_group(world, table, "Materials");
    for row in [
        (
            "Logs",
            Icon::Logs,
            "105",
            "1",
            Icon::Unchanged,
            "steady",
            false,
            false,
            "Default",
        ),
        (
            "Planks",
            Icon::Planks,
            "11",
            "2",
            Icon::TrendUp,
            "up 7",
            true,
            true,
            "Default",
        ),
        (
            "Ore",
            Icon::Ore,
            "24",
            "2",
            Icon::TrendDown,
            "down 4",
            false,
            false,
            "Focus",
        ),
    ] {
        table_line(world, table, &columns, row);
    }
    let detail = w::card(world, table, 10.0);
    w::label(
        world,
        detail,
        "Planks: Harbour 6 · Alderwood 5 · Stone Ridge 0",
        t::DENSE,
        Tone::text(Surface::Paper),
    );
    w::paragraph(
        world,
        detail,
        "Made by 3 carpenters. Two sites still need 13.",
        t::DENSE,
        Tone::secondary(Surface::Paper),
    );

    let cards = block(world, body, "Cards, chips, badges", Surface::Paper, 32.0);
    let row = line(world, cards);
    for (name, state, on, unavailable) in [
        ("Cottage", "Default", false, false),
        ("Family house", "Hover", false, false),
        ("Barn", "Default", true, false),
        ("Lodging house", "Default", false, true),
    ] {
        let card = w::card_button(world, row, name, 8.0, Reach::Ring, ());
        world.get_mut::<Node>(card).expect("card").width = px(150);
        let head = line(world, card);
        let picture = icon::icon(world, head, Icon::Cottage, 24.0, Tone::text(Surface::Paper));
        world.entity_mut(picture).insert(w::Part::Label);
        let text = w::label(world, head, name, t::BODY, Tone::text(Surface::Paper));
        world.entity_mut(text).insert(w::Part::Label);
        let cost = line(world, card);
        for (picture, count) in [(Icon::Planks, "8"), (Icon::Logs, "4")] {
            let picture = icon::icon(world, cost, picture, 14.0, Tone::secondary(Surface::Paper));
            world.entity_mut(picture).insert(w::Part::Secondary);
            let text = w::label(world, cost, count, t::DENSE, Tone::text(Surface::Paper));
            world.entity_mut(text).insert(w::Part::Label);
        }
        let purpose = if unavailable {
            "Needs a carpenter in the village."
        } else {
            "A home with 2 beds"
        };
        let text = w::paragraph(
            world,
            card,
            purpose,
            t::CAPTION,
            Tone::secondary(Surface::Paper),
        );
        world.entity_mut(text).insert(w::Part::Secondary);
        forced(world, card, state);
        w::set_state(world, card, on, unavailable);
        if on {
            w::badge(world, card, Surface::Paper, Status::Good, "In hand");
        }
    }
    let row = line(world, cards);
    for (name, picture, value) in [
        ("Residents", Icon::Residents, "42"),
        ("Housed", Icon::Homes, "42 of 42"),
        ("Beds", Icon::Rest, "84"),
        ("Morale", Icon::Morale, "97%"),
    ] {
        let chip = w::stat_chip(
            world,
            row,
            name,
            picture,
            value,
            Some(name),
            Surface::Paper,
            Reach::Ring,
            (),
        );
        let _ = chip;
    }
    for surface in [Surface::Paper, Surface::Chrome] {
        let holder = if surface == Surface::Chrome {
            let dark = world
                .spawn((
                    ChildOf(cards),
                    w::Skin::ChromeSolid,
                    Node {
                        padding: UiRect::axes(px(10), px(8)),
                        border: UiRect::all(px(1)),
                        border_radius: BorderRadius::all(px(t::RADIUS_MD)),
                        ..default()
                    },
                    BackgroundColor(t::BARK_900),
                    BorderColor::all(t::BARK_600),
                ))
                .id();
            line(world, dark)
        } else {
            line(world, cards)
        };
        for (status, word) in [
            (Status::Good, "Doing fine"),
            (Status::Warn, "Low"),
            (Status::Crit, "In danger: food"),
            (Status::Info, "36 notes"),
        ] {
            w::badge(world, holder, surface, status, word);
        }
        if surface == Surface::Chrome {
            w::count_badge(world, holder, "1");
            w::count_badge(world, holder, "12");
        }
    }
    let empty = w::paragraph(
        world,
        cards,
        "No building is under way. Choose Homes below to place one.",
        t::DENSE,
        Tone::secondary(Surface::Paper),
    );
    let _ = empty;
}

type TableLine<'a> = (
    &'a str,
    Icon,
    &'a str,
    &'a str,
    Icon,
    &'a str,
    bool,
    bool,
    &'a str,
);

fn table_line(world: &mut World, table: Entity, columns: &[(&str, f32); 4], row: TableLine) {
    let (name, picture, stock, price, trend, word, low, on, state) = row;
    let entity = w::table_row(world, table, name, Reach::Ring, ());
    let first = w::table_cell(world, entity, columns[0].1, true);
    icon::icon(world, first, picture, 16.0, Tone::secondary(Surface::Paper));
    w::label(world, first, name, t::DENSE, Tone::text(Surface::Paper));
    if low {
        w::badge(world, first, Surface::Paper, Status::Warn, "Low");
    }
    for (value, width) in [(stock, columns[1].1), (price, columns[2].1)] {
        let cell = w::table_cell(world, entity, width, false);
        w::label(world, cell, value, t::DENSE, Tone::text(Surface::Paper));
    }
    let cell = w::table_cell(world, entity, columns[3].1, false);
    let status = match trend {
        Icon::TrendUp => Status::Good,
        Icon::TrendDown => Status::Crit,
        _ => Status::Idle,
    };
    icon::icon(
        world,
        cell,
        trend,
        12.0,
        Tone::status(Surface::Paper, status),
    );
    w::label(world, cell, word, t::DENSE, Tone::text(Surface::Paper));
    forced(world, entity, state);
    w::set_state(world, entity, on, false);
}

fn messages(world: &mut World, body: Entity) {
    let tips = block(
        world,
        body,
        "Tooltip: level 1, level 2",
        Surface::Paper,
        32.0,
    );
    let tip = Tooltip::new("Planks", "Running low: two building sites need 13.")
        .icon(Icon::Planks)
        .value("11 in the villages")
        .key("F2")
        .detail([
            "Harbour 6 · Alderwood 5 · Stone Ridge 0",
            "Sawn from logs by carpenters, 4 per log.",
            "Click: open Economy",
        ]);
    tooltip::card(world, Some(tips), &tip, 1);
    tooltip::card(world, Some(tips), &tip, 2);
    let warmth = Tooltip::new("Warmth", "Low. They will look for a coat or a lit hearth.")
        .icon(Icon::Warmth)
        .value("41%");
    tooltip::card(world, Some(tips), &warmth, 1);
    let menu = Tooltip::new("Menu", "").key("Esc");
    tooltip::card(world, Some(tips), &menu, 1);

    let toasts = block(world, body, "Toast, confirm", Surface::Paper, 32.0);
    let row = line(world, toasts);
    w::toast(world, row, Status::Good, "Island saved", Some(Icon::Done));
    w::toast(world, row, Status::Info, "Cottage finished", None);
    let failed = w::toast(
        world,
        toasts,
        Status::Crit,
        "Could not save: the disk is full",
        None,
    );
    w::button(
        world,
        failed,
        ButtonSpec::new("Try again", Kind::Secondary, Surface::Chrome)
            .size(Size::Small)
            .reach(Reach::Ring),
        (),
    );
    let row = line(world, toasts);
    w::toast(
        world,
        row,
        Status::Warn,
        "Clean water is low in Stone Ridge",
        None,
    );
    let (_, buttons) = w::inline_confirm(
        world,
        toasts,
        "Abandon Harbour cottage 3?",
        "Delivered materials go to the nearest store. Coins already paid stay paid.",
    );
    w::button(
        world,
        buttons,
        ButtonSpec::new("Keep building", Kind::Secondary, Surface::Paper)
            .size(Size::Small)
            .reach(Reach::Ring),
        (),
    );
    w::button(
        world,
        buttons,
        ButtonSpec::new("Abandon it", Kind::Danger, Surface::Paper)
            .size(Size::Small)
            .reach(Reach::Ring),
        (),
    );
    w::reason(
        world,
        toasts,
        Surface::Paper,
        "A disabled control keeps its place and says what is missing.",
    );

    let modal = block(world, body, "Modal over a scrim", Surface::Paper, 32.0);
    let scrim = world
        .spawn((
            ChildOf(modal),
            Node {
                padding: UiRect::all(px(16)),
                border_radius: BorderRadius::all(px(t::RADIUS_MD)),
                justify_content: JustifyContent::Center,
                ..default()
            },
            BackgroundColor(t::over(t::SCRIM, Color::srgb(0.44, 0.63, 0.29))),
        ))
        .id();
    let panel = w::modal_panel(
        world,
        scrim,
        "Paused",
        "Ada Traveller · Autumn, day 6 · saved 2 minutes ago",
    );
    world.entity_mut(panel).remove::<GlobalZIndex>();
    world.get_mut::<Node>(panel).expect("modal").width = percent(100);
    w::button(
        world,
        panel,
        ButtonSpec::new("Resume", Kind::Primary, Surface::Paper)
            .key("Esc")
            .reach(Reach::Ring),
        (),
    );
    w::button(
        world,
        panel,
        ButtonSpec::new("Save game", Kind::Secondary, Surface::Paper)
            .key("Ctrl+S")
            .reach(Reach::Ring),
        (),
    );
    w::divider(world, panel, Surface::Paper, false);
    w::button(
        world,
        panel,
        ButtonSpec::new("Save and go to title", Kind::Quiet, Surface::Paper).reach(Reach::Ring),
        (),
    );
}

fn icons(world: &mut World, body: Entity) {
    let all = block(
        world,
        body,
        "Icons by concept · Maki and Temaki (CC0) and nine drawn glyphs",
        Surface::Paper,
        100.0,
    );
    let grid = world
        .spawn((
            ChildOf(all),
            Node {
                flex_wrap: FlexWrap::Wrap,
                column_gap: px(4),
                row_gap: px(3),
                ..default()
            },
        ))
        .id();
    for concept in Icon::ALL {
        let cell = world
            .spawn((
                ChildOf(grid),
                Node {
                    width: px(118),
                    height: px(22),
                    column_gap: px(5),
                    align_items: AlignItems::Center,
                    overflow: Overflow::clip_x(),
                    ..default()
                },
            ))
            .id();
        let glyph = concept.glyph();
        icon::icon(world, cell, *concept, 16.0, Tone::text(Surface::Paper));
        // A closest-match glyph always carries its name: marked as a caption.
        let tone = if glyph.alone {
            Tone::text(Surface::Paper)
        } else {
            Tone::new(Surface::Paper, Role::Link)
        };
        w::label(world, cell, concept.name(), t::CAPTION, tone);
    }
    w::reason(
        world,
        all,
        Surface::Paper,
        "Blue names are closest-match glyphs: they never appear without their label.",
    );
    let sizes = block(
        world,
        body,
        "Sizes 12 to 32, on chrome",
        Surface::Chrome,
        49.0,
    );
    let row = line(world, sizes);
    for size in t::ICON_SIZES {
        for concept in [
            Icon::Homes,
            Icon::Planks,
            Icon::Coins,
            Icon::Clear,
            Icon::Done,
        ] {
            icon::icon(world, row, concept, size, Tone::text(Surface::Chrome));
        }
    }
    let drawn = block(
        world,
        body,
        "Drawn in code, with the status shapes",
        Surface::Paper,
        49.0,
    );
    let row = line(world, drawn);
    for kind in Drawn::ALL {
        let concept = Icon::ALL
            .iter()
            .copied()
            .find(|icon| icon.glyph().source == Source::Drawn(kind))
            .expect("every drawn glyph is a concept");
        icon::icon(world, row, concept, 24.0, Tone::text(Surface::Paper));
    }
    icon::icon(world, row, Icon::Storm, 24.0, Tone::text(Surface::Paper));
    for status in [
        Status::Good,
        Status::Warn,
        Status::Crit,
        Status::Info,
        Status::Idle,
    ] {
        icon::icon(
            world,
            row,
            Icon::status(status),
            24.0,
            Tone::status(Surface::Paper, status),
        );
    }
    icon::icon(
        world,
        row,
        Icon::Caution,
        24.0,
        Tone::status(Surface::Paper, Status::Warn),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_page_builds_and_the_style_switch_swaps_the_table() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        install(&mut app, Page::Controls);
        app.update();
        let count = |app: &mut App| {
            app.world_mut()
                .query::<&w::Control>()
                .iter(app.world())
                .count()
        };
        assert!(count(&mut app) > 60, "controls in every state");
        for page in Page::ALL {
            app.world_mut().resource_mut::<Gallery>().page = page;
            app.update();
            let texts = app.world_mut().query::<&Text>().iter(app.world()).count();
            assert!(texts > 20, "{page:?} shows something");
        }
        // The Icons page lists every concept by name.
        let names: Vec<String> = app
            .world_mut()
            .query::<&Text>()
            .iter(app.world())
            .map(|text| text.0.clone())
            .collect();
        for concept in Icon::ALL {
            assert!(
                names.iter().any(|name| name == concept.name()),
                "{concept:?}"
            );
        }
        let dusk = app
            .world_mut()
            .query::<(Entity, &Action)>()
            .iter(app.world())
            .find(|(_, action)| **action == Action::Style(PanelStyle::Dusk))
            .map(|(entity, _)| entity)
            .unwrap();
        app.world_mut().trigger(Activate { entity: dusk });
        app.update();
        assert_eq!(app.world().resource::<Theme>().style, PanelStyle::Dusk);
        assert_eq!(Page::parse("icons"), Some(Page::Icons));
        assert_eq!(Page::parse("nothing"), None);
    }
}
