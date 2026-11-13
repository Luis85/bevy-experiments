//! The build tray and the tag that follows the cursor. Widgets only: they
//! queue [`Action`]s for the tool state and record nothing themselves.
//!
//! A category on the build dock (or B) opens the tray above it: one card per
//! building with its cost as icons, and beside the cards the village's stock
//! of what the pointed-at or held building costs. With a building in hand a
//! tag beside the cursor gives the verdict as a tick or a cross and a
//! sentence, the cost and the keys.
use super::model::{self, Group, PaletteItem, Tag};
use super::{Action, BuildTool};
use crate::game_ui::icon::{self, Icon};
use crate::game_ui::scale;
use crate::game_ui::tokens::{self as t, Status, Surface};
use crate::game_ui::tooltip::Tooltip;
use crate::game_ui::widgets::{self as w, Part, Reach, Role, Swatch, Tone};
use bevy::input::mouse::{MouseScrollUnit, MouseWheel};
use bevy::picking::{Pickable, hover::Hovered};
use bevy::prelude::*;
use bevy::ui::InteractionDisabled;
use bevy::ui_widgets::Activate;
use npc_framework::view_helpers::ViewSystems;

/// The tray at the spec's size; narrower windows shrink it to fit.
const WIDTH: f32 = 816.0;
/// Ten more than the spec's 148: room for the rail under a row that scrolls.
const HEIGHT: f32 = 158.0;
const CARD: (f32, f32) = (188.0, 96.0);
const STOCK_WIDTH: f32 = 196.0;
/// Seconds between two reads of the village stock.
const STOCK_SECS: f32 = 0.25;

/// The build tray's root. (The name predates the tray.)
#[derive(Component)]
pub(crate) struct PaletteRoot;
#[derive(Component)]
struct TrayCards;
#[derive(Component)]
struct TrayStock;
#[derive(Component)]
struct TrayTitle;
#[derive(Component)]
struct TrayIcon;
#[derive(Component)]
struct TraySentence;
#[derive(Component)]
struct ItemCard(String);
/// The "In hand" badge of a card.
#[derive(Component)]
struct HandBadge(String);
#[derive(Component, Clone)]
pub(super) struct BuildAction(pub(super) Action);
#[derive(Component)]
struct CursorTag;

/// What the tray last showed, so each part is rebuilt only when it changes.
#[derive(Resource, Default)]
struct Shown {
    listed: Option<(u64, Group)>,
    stock: Option<String>,
    tag: Option<Tag>,
    elapsed: f32,
}

pub(super) fn plugin(app: &mut App) {
    crate::panel_scroll::install(app);
    app.init_resource::<Shown>()
        .add_observer(activate)
        // A windowed host has this already; a headless one may not.
        .add_message::<MouseWheel>()
        .add_systems(Startup, spawn)
        .add_systems(
            Update,
            (
                wheel.in_set(ViewSystems::OverlayInput),
                refresh.in_set(ViewSystems::Present).after(super::process),
            ),
        );
}

/// Logical pixels one wheel line moves the row of cards.
const WHEEL_LINE: f32 = 64.0;

/// How far the wheel moves the row: down or right is forward, like a page.
pub(crate) fn wheel_step(unit: MouseScrollUnit, x: f32, y: f32) -> f32 {
    let step = match unit {
        MouseScrollUnit::Line => WHEEL_LINE,
        MouseScrollUnit::Pixel => 1.0,
    };
    (x - y) * step
}

/// The wheel over the cards moves the row sideways. It writes the view's own
/// scroll position and nothing else.
fn wheel(
    mut turns: MessageReader<MouseWheel>,
    mut rows: Query<(&Hovered, &ComputedNode, &mut ScrollPosition), With<TrayCards>>,
) {
    let delta: f32 = turns
        .read()
        .map(|turn| wheel_step(turn.unit, turn.x, turn.y))
        .sum();
    if delta == 0.0 {
        return;
    }
    for (hovered, node, mut scroll) in &mut rows {
        if hovered.get() {
            let most = crate::panel_scroll::overflow_x(node);
            scroll.x = (scroll.x + delta).clamp(0.0, most);
        }
    }
}

/// The build tray for a test that walks the Tab ring.
#[cfg(test)]
pub(crate) fn spawn_for_tests(world: &mut World) {
    world.init_resource::<Shown>();
    spawn(world);
}

fn spawn(world: &mut World) {
    let root = world
        .spawn((
            Name::new("Build tray"),
            PaletteRoot,
            crate::game_ui::sound::PanelSound::default(),
            crate::island_shell::focus::FocusRegion::Build,
            GlobalZIndex(t::layer::PANEL),
            w::panel(Node {
                position_type: PositionType::Absolute,
                left: px(232),
                bottom: px(t::SCREEN_MARGIN + t::DOCK + t::SCREEN_MARGIN),
                width: px(WIDTH),
                height: px(HEIGHT),
                padding: UiRect::axes(px(12), px(8)),
                row_gap: px(6),
                flex_direction: FlexDirection::Column,
                display: Display::None,
                ..default()
            }),
        ))
        .id();
    let header = world
        .spawn((
            ChildOf(root),
            Node {
                height: px(28),
                min_height: px(28),
                align_items: AlignItems::Center,
                column_gap: px(8),
                flex_shrink: 0.0,
                ..default()
            },
        ))
        .id();
    world.spawn((
        ChildOf(header),
        TrayIcon,
        Node {
            width: px(20),
            height: px(20),
            flex_shrink: 0.0,
            ..default()
        },
    ));
    let title = w::label(world, header, "", t::SUBHEADING, Tone::text(Surface::Paper));
    world.entity_mut(title).insert(TrayTitle);
    let sentence = w::label(world, header, "", t::CAPTION, Tone::caption(Surface::Paper));
    world.entity_mut(sentence).insert((
        TraySentence,
        Node {
            flex_grow: 1.0,
            flex_shrink: 1.0,
            min_width: px(0),
            overflow: Overflow::clip_x(),
            ..default()
        },
    ));
    let close = w::icon_button(
        world,
        header,
        "Close build menu",
        Icon::Close,
        Surface::Paper,
        Reach::Ring,
        BuildAction(Action::Close),
    );
    {
        let mut node = world.get_mut::<Node>(close).expect("close");
        node.width = px(28);
        node.height = px(28);
        node.min_width = px(28);
    }
    world
        .entity_mut(close)
        .insert(Tooltip::new("Close", "Close the build menu.").key("B"));
    let content = world
        .spawn((
            ChildOf(root),
            Node {
                flex_grow: 1.0,
                min_height: px(0),
                column_gap: px(10),
                ..default()
            },
        ))
        .id();
    let cards = world
        .spawn((
            ChildOf(content),
            TrayCards,
            Hovered::default(),
            // One row; more cards than fit scroll sideways with the wheel
            // and the rail under them.
            Node {
                column_gap: px(6),
                align_items: AlignItems::FlexStart,
                flex_grow: 1.0,
                min_height: px(0),
                overflow: Overflow::scroll_x(),
                ..default()
            },
        ))
        .id();
    crate::panel_scroll::attach_rail_x(world, content, cards, "Build");
    world.spawn((
        ChildOf(content),
        TrayStock,
        Swatch::Edge,
        Tone::new(Surface::Paper, Role::Edge),
        Node {
            width: px(STOCK_WIDTH),
            min_width: px(STOCK_WIDTH),
            padding: UiRect::left(px(10)),
            border: UiRect::left(px(1)),
            row_gap: px(3),
            flex_direction: FlexDirection::Column,
            flex_shrink: 0.0,
            ..default()
        },
        BorderColor::all(Color::NONE),
    ));
    world.spawn((
        Name::new("Build cursor tag"),
        CursorTag,
        Pickable::IGNORE,
        GlobalZIndex(t::layer::TOOLTIP),
        Node {
            position_type: PositionType::Absolute,
            left: px(0),
            top: px(0),
            width: px(250),
            padding: UiRect::axes(px(10), px(8)),
            border: UiRect::all(px(1)),
            border_radius: BorderRadius::all(px(t::RADIUS_MD)),
            flex_direction: FlexDirection::Column,
            row_gap: px(4),
            display: Display::None,
            ..default()
        },
        BackgroundColor(t::BARK_900),
        BorderColor::all(t::BARK_600),
        t::shadow(t::SHADOW_2),
    ));
}

fn activate(
    event: On<Activate>,
    actions: Query<(&BuildAction, Has<InteractionDisabled>)>,
    mut tool: ResMut<BuildTool>,
) {
    if let Ok((action, false)) = actions.get(event.entity) {
        tool.requests.push(action.0.clone());
    }
}

fn set_text<M: Component>(world: &mut World, value: &str) {
    for mut text in world.query_filtered::<&mut Text, With<M>>().iter_mut(world) {
        if text.0 != value {
            text.0 = value.into();
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NoteSide {
    Left,
    Right,
}

/// Which side of the cursor its tag goes: right of it in the left part of
/// the window, left of it from the middle on, where the right column stands.
pub(crate) fn note_side(cursor_x: f32, window_width: f32) -> NoteSide {
    if cursor_x >= window_width * 0.48 {
        NoteSide::Left
    } else {
        NoteSide::Right
    }
}

/// Where the tray sits: centred above the docks, never wider than the window.
pub(crate) fn tray_rect(window_width: f32) -> (f32, f32) {
    let width = WIDTH.min(window_width - 2.0 * t::SCREEN_MARGIN).max(320.0);
    (((window_width - width) * 0.5).round(), width)
}

/// One card per building of the shown group: icon and name, the cost as
/// icon and number pairs, one line of purpose.
fn list(world: &mut World, group: Group) {
    let Some(body) = world
        .query_filtered::<Entity, With<TrayCards>>()
        .iter(world)
        .next()
    else {
        return;
    };
    world.entity_mut(body).despawn_children();
    let items: Vec<PaletteItem> = world
        .resource::<BuildTool>()
        .items
        .iter()
        .filter(|item| item.group == group)
        .cloned()
        .collect();
    for item in items {
        let card = w::card_button(
            world,
            body,
            &item.name,
            8.0,
            Reach::Ring,
            (
                BuildAction(Action::Pick(item.id.clone())),
                ItemCard(item.id.clone()),
            ),
        );
        {
            let mut node = world.get_mut::<Node>(card).expect("card");
            node.width = px(CARD.0);
            node.min_width = px(CARD.0);
            node.flex_shrink = 0.0;
            node.min_height = px(CARD.1);
            node.row_gap = px(3);
        }
        world.entity_mut(card).insert(
            // An unavailable card says why first, in the same words as its line.
            Tooltip::new(item.name.clone(), item.card_note().to_owned())
                .icon(item.icon)
                .detail([
                    model::capitalised_cost(&item.cost),
                    format!(
                        "{} \u{d7} {} m behind its entrance.",
                        item.size.0, item.size.1
                    ),
                ]),
        );
        let head = world
            .spawn((
                ChildOf(card),
                Node {
                    align_items: AlignItems::Center,
                    column_gap: px(6),
                    ..default()
                },
            ))
            .id();
        let picture = icon::icon(world, head, item.icon, 24.0, Tone::text(Surface::Paper));
        world.entity_mut(picture).insert(Part::Label);
        let name = w::label(world, head, &item.name, t::BODY, Tone::text(Surface::Paper));
        world.entity_mut(name).insert((
            Part::Label,
            Node {
                flex_grow: 1.0,
                flex_shrink: 1.0,
                min_width: px(0),
                overflow: Overflow::clip_x(),
                ..default()
            },
        ));
        // "In hand": a tick and the words, shown while this is the building held.
        let hand = w::badge(world, head, Surface::Paper, Status::Good, "In hand");
        world
            .entity_mut(hand)
            .insert((HandBadge(item.id.clone()), Pickable::IGNORE));
        w::show(world, hand, false);
        let cost = world
            .spawn((
                ChildOf(card),
                Node {
                    align_items: AlignItems::Center,
                    column_gap: px(4),
                    ..default()
                },
            ))
            .id();
        if item.materials.is_empty() {
            let text = w::label(
                world,
                cost,
                "No materials",
                t::DENSE,
                Tone::secondary(Surface::Paper),
            );
            world.entity_mut(text).insert(Part::Secondary);
        }
        for (kind, count) in &item.materials {
            if let Some(picture) = Icon::of_good(kind) {
                let picture =
                    icon::icon(world, cost, picture, 14.0, Tone::secondary(Surface::Paper));
                world.entity_mut(picture).insert(Part::Secondary);
            }
            // A closest-match glyph never stands alone: the name is with it.
            let text = w::label(
                world,
                cost,
                crate::game_ui::words::count(u64::from(*count), &super::good_name(world, kind)),
                t::DENSE,
                Tone::text(Surface::Paper),
            );
            world.entity_mut(text).insert((
                Part::Label,
                Node {
                    margin: UiRect::right(px(4)),
                    flex_shrink: 0.0,
                    ..default()
                },
            ));
        }
        if item.unavailable.is_some() {
            w::reason(world, card, Surface::Paper, item.card_note());
        } else {
            let purpose = w::paragraph(
                world,
                card,
                item.card_note(),
                t::CAPTION,
                Tone::secondary(Surface::Paper),
            );
            world.entity_mut(purpose).insert(Part::Secondary);
        }
    }
    for mut scroll in world
        .query_filtered::<&mut ScrollPosition, With<TrayCards>>()
        .iter_mut(world)
    {
        scroll.0 = Vec2::ZERO;
    }
}

/// The village's stock of what one building costs, and the civic fund.
fn stock_lines(
    world: &World,
    item: Option<&PaletteItem>,
) -> Vec<(Option<Icon>, String, String, Status)> {
    let mut lines = Vec::new();
    if let Some(item) = item {
        let totals = crate::colony_panel::stock_totals(world);
        for (kind, count) in &item.materials {
            let held = totals.get(&(None, kind.clone())).copied().unwrap_or(0);
            lines.push((
                Icon::of_good(kind),
                crate::colony_panel::capitalised(&super::good_name(world, kind)),
                held.to_string(),
                if held >= *count {
                    Status::Good
                } else {
                    Status::Warn
                },
            ));
        }
    }
    lines.push((
        Some(Icon::Coins),
        "Civic fund to spend".to_owned(),
        island_content::village::civic::budget(world).to_string(),
        Status::Idle,
    ));
    lines
}

fn refresh_stock(world: &mut World, item: Option<PaletteItem>) {
    let lines = stock_lines(world, item.as_ref());
    let title = item.as_ref().map_or_else(
        || "Village stock".to_owned(),
        |item| format!("Village stock for {}", item.name.to_lowercase()),
    );
    let signature = format!("{title}{lines:?}");
    if world.resource::<Shown>().stock.as_ref() == Some(&signature) {
        return;
    }
    let Some(column) = world
        .query_filtered::<Entity, With<TrayStock>>()
        .iter(world)
        .next()
    else {
        return;
    };
    world.entity_mut(column).despawn_children();
    w::label(
        world,
        column,
        title,
        t::CAPTION,
        Tone::caption(Surface::Paper),
    );
    for (picture, name, value, status) in lines {
        let row = world
            .spawn((
                ChildOf(column),
                Node {
                    align_items: AlignItems::Center,
                    column_gap: px(5),
                    ..default()
                },
            ))
            .id();
        if let Some(picture) = picture {
            icon::icon(world, row, picture, 14.0, Tone::secondary(Surface::Paper));
        }
        let name = w::label(world, row, name, t::DENSE, Tone::text(Surface::Paper));
        world.entity_mut(name).insert(Node {
            flex_grow: 1.0,
            flex_shrink: 1.0,
            min_width: px(0),
            overflow: Overflow::clip_x(),
            ..default()
        });
        w::label(world, row, value, t::DENSE, Tone::text(Surface::Paper));
        // Enough is a circle, short a triangle; the fund has no verdict.
        if status != Status::Idle {
            icon::icon(
                world,
                row,
                Icon::status(status),
                12.0,
                Tone::status(Surface::Paper, status),
            );
        }
    }
    world.resource_mut::<Shown>().stock = Some(signature);
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

/// Rebuild the tag when what it says changes: the verdict with a tick or a
/// cross, the simulation's reason, the cost and footprint, then the keys.
fn rebuild_tag(world: &mut World, root: Entity, tag: &Tag) {
    world.entity_mut(root).despawn_children();
    let edge = if tag.valid {
        t::BARK_600
    } else {
        t::CRIT_LIGHT
    };
    if let Some(mut border) = world.get_mut::<BorderColor>(root) {
        *border = BorderColor::all(edge);
    }
    let verdict = world
        .spawn((
            ChildOf(root),
            Node {
                align_items: AlignItems::Center,
                column_gap: px(6),
                ..default()
            },
        ))
        .id();
    let status = if tag.valid {
        Status::Good
    } else {
        Status::Crit
    };
    icon::icon(
        world,
        verdict,
        if tag.valid { Icon::Done } else { Icon::Cannot },
        16.0,
        Tone::status(Surface::Chrome, status),
    );
    w::label(
        world,
        verdict,
        tag.verdict,
        t::DENSE,
        Tone::text(Surface::Chrome),
    );
    if let Some(reason) = &tag.reason {
        w::paragraph(
            world,
            root,
            reason,
            t::DENSE,
            Tone::secondary(Surface::Chrome),
        );
    }
    w::paragraph(
        world,
        root,
        &tag.facts,
        t::DENSE,
        Tone::secondary(Surface::Chrome),
    );
    let keys = world
        .spawn((
            ChildOf(root),
            Node {
                align_items: AlignItems::Center,
                column_gap: px(4),
                flex_wrap: FlexWrap::Wrap,
                row_gap: px(3),
                ..default()
            },
        ))
        .id();
    for (key, verb) in [("Click", "Place"), ("R", "Turn"), ("Esc", "Put down")] {
        w::keycap(world, keys, Surface::Chrome, key);
        let verb = w::label(
            world,
            keys,
            verb,
            t::CAPTION,
            Tone::secondary(Surface::Chrome),
        );
        world.entity_mut(verb).insert(Node {
            margin: UiRect::right(px(6)),
            flex_shrink: 0.0,
            ..default()
        });
    }
    ignore_pointer(world, root);
}

fn refresh(world: &mut World) {
    let shown = super::palette_open(world);
    let layout = scale::layout(world);
    let (left, width) = tray_rect(layout.width);
    let bottom = crate::island_shell::content_bottom(world);
    for mut node in world
        .query_filtered::<&mut Node, With<PaletteRoot>>()
        .iter_mut(world)
    {
        let display = if shown { Display::Flex } else { Display::None };
        if node.display != display {
            node.display = display;
        }
        if node.left != px(left) {
            node.left = px(left);
        }
        if node.width != px(width) {
            node.width = px(width);
        }
        if node.bottom != px(bottom) {
            node.bottom = px(bottom);
        }
    }
    let tags: Vec<Entity> = world
        .query_filtered::<Entity, With<CursorTag>>()
        .iter(world)
        .collect();
    if !shown {
        for tag in tags {
            w::show(world, tag, false);
        }
        return;
    }
    let (group, epoch) = {
        let tool = world.resource::<BuildTool>();
        (
            model::shown_group(&tool.items, tool.group),
            tool.items_epoch.unwrap_or(0),
        )
    };
    if let Some(group) = group
        && world.resource::<Shown>().listed != Some((epoch, group))
    {
        list(world, group);
        let slots: Vec<Entity> = world
            .query_filtered::<Entity, With<TrayIcon>>()
            .iter(world)
            .collect();
        for slot in slots {
            world.entity_mut(slot).despawn_children();
            icon::icon(world, slot, group.icon(), 20.0, Tone::text(Surface::Paper));
        }
        set_text::<TrayTitle>(world, group.label());
        world.resource_mut::<Shown>().listed = Some((epoch, group));
    }
    let tool = world.resource::<BuildTool>();
    let (selected, queued) = (tool.selected.clone(), tool.queued());
    // State, not instruction: what was just placed or refused, or how
    // building works when nothing has happened yet.
    let sentence = if queued > 0 {
        format!(
            "{queued} placement{} waiting for the clock to run.",
            if queued == 1 { "" } else { "s" }
        )
    } else if tool.status.is_empty() {
        "Residents carry the materials and build it; the civic fund pays them.".to_owned()
    } else {
        tool.status.clone()
    };
    let ghost = tool.ghost.clone();
    // The ghost's own refusal first, then the simulation's for a placement.
    let refused = tool.cursor_reason().map(str::to_owned);
    let held = selected
        .as_deref()
        .and_then(|id| tool.items.iter().find(|item| item.id == id))
        .cloned();
    let items = tool.items.clone();
    set_text::<TraySentence>(world, &sentence);
    // Cards: the held building is lit and badged.
    let cards: Vec<(Entity, String, bool)> = world
        .query::<(Entity, &ItemCard, &Hovered)>()
        .iter(world)
        .map(|(entity, card, hovered)| (entity, card.0.clone(), hovered.get()))
        .collect();
    let mut pointed = None;
    for (entity, id, hovered) in cards {
        // Unavailable is a look of its own; the card stays reachable so its
        // reason can be read.
        let unavailable = items
            .iter()
            .any(|item| item.id == id && item.unavailable.is_some());
        w::set_state(
            world,
            entity,
            selected.as_deref() == Some(id.as_str()),
            unavailable,
        );
        if hovered {
            pointed = items.iter().find(|item| item.id == id).cloned();
        }
    }
    let badges: Vec<(Entity, bool)> = world
        .query::<(Entity, &HandBadge)>()
        .iter(world)
        .map(|(entity, badge)| (entity, selected.as_deref() == Some(badge.0.as_str())))
        .collect();
    for (entity, on) in badges {
        w::show(world, entity, on);
    }
    // The stock column follows the pointer, else the building in hand.
    let delta = world.get_resource::<Time>().map_or(0.0, Time::delta_secs);
    let due = {
        let mut state = world.resource_mut::<Shown>();
        state.elapsed += delta;
        let due = state.elapsed >= STOCK_SECS || state.stock.is_none();
        if due {
            state.elapsed = 0.0;
        }
        due
    };
    if due {
        refresh_stock(world, pointed.or_else(|| held.clone()));
    }

    // The tag beside the cursor, while a building is in hand over the island.
    let cursor = world
        .query::<&Window>()
        .iter(world)
        .find_map(Window::cursor_position);
    let tag = match (&ghost, &held) {
        (Some(_), Some(item)) => Some(model::tag(item, refused.as_deref())),
        _ => None,
    };
    for root in tags {
        match (cursor, &tag) {
            (Some(cursor), Some(tag)) => {
                if world.resource::<Shown>().tag.as_ref() != Some(tag) {
                    rebuild_tag(world, root, tag);
                    world.resource_mut::<Shown>().tag = Some(tag.clone());
                }
                let (x, y) = (cursor.x / layout.scale, cursor.y / layout.scale);
                let mut node = world.get_mut::<Node>(root).expect("tag");
                node.display = Display::Flex;
                node.top = px(y + 18.0);
                // Beside the cursor on the side with more room.
                match note_side(x, layout.width) {
                    NoteSide::Right => {
                        node.left = px(x + 18.0);
                        node.right = Val::Auto;
                    }
                    NoteSide::Left => {
                        node.left = Val::Auto;
                        node.right = px(layout.width - x + 18.0);
                    }
                }
            }
            _ => w::show(world, root, false),
        }
    }
}
