//! The Live workspace: one panel in the right column for the open hotbar
//! tile. Rows come from `model::rows`; this module only draws them with the
//! widget kit and keeps each row's entity while its text changes.
use super::*;
use crate::game_ui::icon::{self, Icon};
use crate::game_ui::scale::{self, LayoutClass};
use crate::game_ui::tokens::{self as t, Status, Surface};
use crate::game_ui::tooltip::Tooltip;
use crate::game_ui::widgets::{self as w, ButtonSpec, Kind, Part, Reach, Size, Tone};
use bevy::input_focus::tab_navigation::TabIndex;

/// The workspace's width for a tab and a layout class, and the width of the
/// pouch pane where the tab has two panes (Craft and Trade): 360 and 600
/// (pouch 276) on a wide window, 340 and 540 (pouch 244) at 1024, which
/// leaves the Paused pill room beside the goal tracker. The narrow column
/// has one pane at 320.
pub(super) fn width(tab: Tab, class: LayoutClass) -> (f32, Option<f32>) {
    let two = matches!(tab, Tab::Craft | Tab::Trade);
    match (class, two) {
        (LayoutClass::Compact, _) => (320.0, None),
        (LayoutClass::Wide, true) => (600.0, Some(276.0)),
        (LayoutClass::Wide, false) => (360.0, None),
        (LayoutClass::Standard, true) => (540.0, Some(244.0)),
        (LayoutClass::Standard, false) => (340.0, None),
    }
}

/// What kind of widget a keyed row is; a key keeps its kind.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
enum RowKind {
    Heading,
    Note,
    Item,
    Compact,
    Action,
    /// A detail card with its own buttons.
    Card,
}

fn kind_of(row: &model::Row) -> RowKind {
    if row.key == "item_detail" || row.card {
        RowKind::Card
    } else if row.heading {
        RowKind::Heading
    } else if row.item {
        RowKind::Item
    } else if row.action.is_some() && row.compact {
        RowKind::Compact
    } else if row.action.is_some() {
        RowKind::Action
    } else {
        RowKind::Note
    }
}

/// The parts of a row a refresh rewrites in place.
#[derive(Component, Clone, Debug, Default)]
struct RowParts {
    title: Option<Entity>,
    detail: Option<Entity>,
    /// Holds the input chips of a recipe, and what it shows.
    chips: Option<Entity>,
    chips_shown: Vec<(String, u64, u64)>,
    /// Holds the reason under a disabled row, and what it says.
    reason: Option<Entity>,
    reason_shown: String,
    /// Holds the picture of an item, and the good it shows.
    picture: Option<Entity>,
    picture_shown: String,
    /// Holds the buttons of a detail card, and what they were built from.
    buttons: Option<Entity>,
    buttons_shown: Vec<(String, bool, String)>,
    /// A bar under the row (the pack's weight, work left).
    meter: Option<w::Bar>,
    /// Pips under the row (a bond's tier).
    pips: Option<Entity>,
}

/// The bar and the pip row every text row may carry, hidden until used.
fn extras(world: &mut World, entity: Entity, parts: &mut RowParts, surface: Surface) {
    let bar = w::progress_bar(world, entity, surface, 0.0, Some(""));
    if let Some(mut node) = world.get_mut::<Node>(bar.entity) {
        node.width = percent(100);
        node.display = Display::None;
    }
    parts.meter = Some(bar);
    let pips = w::pips(world, entity);
    if let Some(mut node) = world.get_mut::<Node>(pips) {
        node.display = Display::None;
    }
    parts.pips = Some(pips);
}

#[derive(Component)]
pub(super) struct WorkspaceIcon;
#[derive(Component)]
pub(super) struct WorkspaceTitle;
#[derive(Component)]
pub(super) struct WorkspaceBadge;
/// The pouch pane of a two-pane workspace, and the rule beside it.
#[derive(Component)]
pub(super) struct PouchPane;
#[derive(Component)]
pub(super) struct PouchSlots;

pub(super) fn spawn(world: &mut World) {
    let workspace = root(
        world,
        "Player workspace",
        w::panel(Node {
            position_type: PositionType::Absolute,
            right: px(t::SCREEN_MARGIN),
            top: px(t::TOP_BAR + t::SCREEN_MARGIN),
            bottom: px(t::SCREEN_MARGIN + t::DOCK + t::SCREEN_MARGIN),
            width: px(360),
            max_width: percent(94),
            row_gap: px(8),
            flex_direction: FlexDirection::Column,
            display: Display::None,
            ..default()
        }),
    );
    world.entity_mut(workspace).insert((
        WorkspaceRoot,
        GlobalZIndex(t::layer::PANEL),
        crate::game_ui::sound::PanelSound::default(),
    ));
    let header = world
        .spawn((
            ChildOf(workspace),
            // Icon, name and badge; the badge goes to a second line where
            // the column is too narrow for it. Close keeps the corner.
            Node {
                align_items: AlignItems::Center,
                flex_wrap: FlexWrap::Wrap,
                column_gap: px(8),
                row_gap: px(4),
                min_height: px(32),
                padding: UiRect::right(px(t::ICON_BUTTON + 8.0)),
                flex_shrink: 0.0,
                ..default()
            },
        ))
        .id();
    world.spawn((
        ChildOf(header),
        WorkspaceIcon,
        Node {
            width: px(20),
            height: px(20),
            flex_shrink: 0.0,
            ..default()
        },
    ));
    let title = w::label(world, header, "", t::HEADING, Tone::text(Surface::Paper));
    world.entity_mut(title).insert((
        WorkspaceTitle,
        Node {
            flex_shrink: 0.0,
            ..default()
        },
    ));
    // The status of the workspace, where the observation has one to give.
    world.spawn((
        ChildOf(header),
        WorkspaceBadge,
        Node {
            flex_shrink: 0.0,
            ..default()
        },
    ));
    let close = w::icon_button(
        world,
        header,
        "Close workspace",
        Icon::Close,
        Surface::Paper,
        Reach::Tab(0),
        (PlayerControl, PlayerAction(Action::Close)),
    );
    world
        .entity_mut(close)
        .insert(Tooltip::new("Close", "Close the workspace.").key("Esc"));
    if let Some(mut node) = world.get_mut::<Node>(close) {
        node.position_type = PositionType::Absolute;
        node.right = px(0);
        node.top = px(0);
    }
    let content = world
        .spawn((
            ChildOf(workspace),
            Node {
                flex_grow: 1.0,
                min_height: px(0),
                column_gap: px(10),
                ..default()
            },
        ))
        .id();
    // Two-pane workspaces: what you carry, beside what you can make or trade.
    let pouch = world
        .spawn((
            ChildOf(content),
            PouchPane,
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: px(6),
                flex_shrink: 0.0,
                padding: UiRect::right(px(10)),
                border: UiRect::right(px(1)),
                display: Display::None,
                ..default()
            },
            w::Swatch::Edge,
            Tone::new(Surface::Paper, w::Role::Edge),
            BorderColor::all(Color::NONE),
        ))
        .id();
    w::label(
        world,
        pouch,
        "Your pouch",
        t::CAPTION,
        Tone::caption(Surface::Paper),
    );
    world.spawn((
        ChildOf(pouch),
        PouchSlots,
        Node {
            flex_wrap: FlexWrap::Wrap,
            column_gap: px(6),
            row_gap: px(6),
            align_content: AlignContent::FlexStart,
            overflow: Overflow::clip_y(),
            flex_grow: 1.0,
            min_height: px(0),
            ..default()
        },
    ));
    let body = world
        .spawn((
            ChildOf(content),
            WorkspaceBody,
            ScrollArea,
            Node {
                flex_direction: FlexDirection::Row,
                flex_wrap: FlexWrap::Wrap,
                align_content: AlignContent::FlexStart,
                column_gap: px(8),
                row_gap: px(6),
                flex_grow: 1.0,
                min_height: px(0),
                overflow: Overflow::scroll_y(),
                ..default()
            },
        ))
        .id();
    crate::panel_scroll::attach_rail(world, content, body, "Player");
}

fn status_chip(world: &mut World, parent: Entity, ok: bool, text: &str) -> Entity {
    let chip = world
        .spawn((
            ChildOf(parent),
            w::Skin::Boxed,
            Node {
                height: px(t::CHIP_PAPER),
                padding: UiRect::horizontal(px(7)),
                column_gap: px(5),
                border: UiRect::all(px(1)),
                border_radius: BorderRadius::all(px(t::RADIUS_SM)),
                align_items: AlignItems::Center,
                flex_shrink: 0.0,
                ..default()
            },
            BackgroundColor(Color::NONE),
            BorderColor::all(Color::NONE),
        ))
        .id();
    let status = if ok { Status::Good } else { Status::Crit };
    // A tick for what you have, a diamond for what is missing.
    icon::icon(
        world,
        chip,
        if ok { Icon::Done } else { Icon::status(status) },
        12.0,
        Tone::status(Surface::Paper, status),
    );
    w::label(world, chip, text, t::DENSE, Tone::text(Surface::Paper));
    chip
}

/// The tooltip of a workspace row: what it does, then what it takes and why
/// it cannot be done now, where there is either.
pub(super) fn row_tip(row: &model::Row) -> Tooltip {
    let mut lines = Vec::new();
    if !row.inputs.is_empty() {
        lines.push(format!(
            "Takes {}.",
            row.inputs
                .iter()
                .map(|(name, _, wanted)| format!("{wanted} {}", name.to_lowercase()))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    if !row.enabled && !row.reason.is_empty() {
        lines.push(row.reason.clone());
    }
    let body = if row.detail.is_empty() {
        "Choose it to do this.".to_owned()
    } else {
        row.detail.clone()
    };
    let tip = Tooltip::new(row.title.clone(), body);
    if lines.is_empty() {
        tip
    } else {
        tip.detail(lines)
    }
}

/// The tooltip of a recipe's input chip.
pub(super) fn chip_tip(name: &str, owned: u64, wanted: u64) -> Tooltip {
    Tooltip::new(
        name,
        if owned >= wanted {
            format!("You have {owned}; this takes {wanted}.")
        } else {
            format!(
                "You have {owned}; this takes {wanted}. {} more to find.",
                wanted - owned
            )
        },
    )
}

/// The good an item row is about, from its key.
fn item_kind(key: &str) -> Option<&str> {
    key.strip_prefix("item_")
        .or_else(|| key.strip_prefix("pouch_"))
        .or_else(|| key.strip_prefix("gift_"))
}

fn build(world: &mut World, body: Entity, row: &model::Row, index: usize) -> Entity {
    let kind = kind_of(row);
    let mut parts = RowParts::default();
    let reach = Reach::Tab(100 + index as i32);
    let action = row.action.clone();
    let entity = match kind {
        RowKind::Card => {
            // The chosen item's card: name, how many, then what to do.
            let entity = w::card(world, body, 10.0);
            if let Some(mut node) = world.get_mut::<Node>(entity) {
                node.width = percent(100);
                node.flex_direction = FlexDirection::Column;
                node.row_gap = px(6);
                node.flex_shrink = 0.0;
            }
            parts.title = Some(w::label(
                world,
                entity,
                &row.title,
                t::SUBHEADING,
                Tone::text(Surface::Paper),
            ));
            parts.detail = Some(w::paragraph(
                world,
                entity,
                &row.detail,
                t::CAPTION,
                Tone::secondary(Surface::Paper),
            ));
            // What it takes and how far it is, then what can be done.
            parts.chips = Some(
                world
                    .spawn((
                        ChildOf(entity),
                        Node {
                            width: percent(100),
                            flex_wrap: FlexWrap::Wrap,
                            column_gap: px(5),
                            row_gap: px(4),
                            display: Display::None,
                            ..default()
                        },
                    ))
                    .id(),
            );
            extras(world, entity, &mut parts, Surface::Paper);
            parts.buttons = Some(
                world
                    .spawn((
                        ChildOf(entity),
                        Node {
                            width: percent(100),
                            flex_direction: FlexDirection::Column,
                            row_gap: px(6),
                            ..default()
                        },
                    ))
                    .id(),
            );
            entity
        }
        RowKind::Heading => {
            let entity = world
                .spawn((
                    ChildOf(body),
                    Node {
                        width: percent(100),
                        margin: UiRect::top(px(6)),
                        flex_shrink: 0.0,
                        ..default()
                    },
                ))
                .id();
            parts.title = Some(w::label(
                world,
                entity,
                &row.title,
                t::SUBHEADING,
                Tone::text(Surface::Paper),
            ));
            entity
        }
        RowKind::Note => {
            let entity = world
                .spawn((
                    ChildOf(body),
                    Node {
                        width: percent(100),
                        flex_direction: FlexDirection::Column,
                        row_gap: px(2),
                        flex_shrink: 0.0,
                        ..default()
                    },
                ))
                .id();
            parts.title = Some(w::paragraph(
                world,
                entity,
                &row.title,
                t::BODY,
                Tone::text(Surface::Paper),
            ));
            parts.detail = Some(w::paragraph(
                world,
                entity,
                &row.detail,
                t::CAPTION,
                Tone::secondary(Surface::Paper),
            ));
            extras(world, entity, &mut parts, Surface::Paper);
            entity
        }
        RowKind::Item => {
            // A slot: the good's picture and how many, its name beneath.
            let entity = w::card_button(
                world,
                body,
                &row.title,
                4.0,
                reach,
                (
                    PlayerControl,
                    PlayerAction(action.expect("an item is chosen")),
                ),
            );
            {
                let mut node = world.get_mut::<Node>(entity).expect("slot");
                node.width = px(76);
                node.min_height = px(72);
                node.align_items = AlignItems::Center;
                node.row_gap = px(1);
            }
            let top = world
                .spawn((
                    ChildOf(entity),
                    Node {
                        align_items: AlignItems::Center,
                        column_gap: px(4),
                        height: px(30),
                        ..default()
                    },
                ))
                .id();
            parts.picture = Some(
                world
                    .spawn((
                        ChildOf(top),
                        Node {
                            width: px(24),
                            height: px(24),
                            ..default()
                        },
                    ))
                    .id(),
            );
            let count = w::label(world, top, &row.detail, t::BODY, Tone::text(Surface::Paper));
            world.entity_mut(count).insert(Part::Label);
            parts.detail = Some(count);
            let name = w::label(
                world,
                entity,
                &row.title,
                t::CAPTION,
                Tone::secondary(Surface::Paper),
            );
            world.entity_mut(name).insert(Part::Secondary);
            parts.title = Some(name);
            entity
        }
        RowKind::Compact => {
            let entity = w::button(
                world,
                body,
                ButtonSpec::new(&row.title, Kind::Secondary, Surface::Paper)
                    .size(Size::Small)
                    .reach(reach),
                (
                    PlayerControl,
                    PlayerAction(action.expect("a compact choice acts")),
                ),
            );
            {
                let mut node = world.get_mut::<Node>(entity).expect("choice");
                // The repeat stepper is three to a line, other choices two.
                node.width = percent(if row.key.starts_with("repeat_") {
                    31
                } else {
                    48
                });
                node.min_width = px(0);
            }
            parts.title = world.get::<Children>(entity).and_then(|children| {
                children
                    .iter()
                    .find(|child| world.get::<Text>(*child).is_some())
            });
            entity
        }
        RowKind::Action => {
            let entity = w::card_button(
                world,
                body,
                &row.title,
                8.0,
                reach,
                (
                    PlayerControl,
                    PlayerAction(action.expect("an action row acts")),
                ),
            );
            {
                let mut node = world.get_mut::<Node>(entity).expect("row");
                node.width = percent(100);
                node.min_height = px(t::TARGET_MIN.1);
                node.row_gap = px(3);
            }
            let title = w::paragraph(
                world,
                entity,
                &row.title,
                t::BODY,
                Tone::text(Surface::Paper),
            );
            world.entity_mut(title).insert(Part::Label);
            parts.title = Some(title);
            let detail = w::paragraph(
                world,
                entity,
                &row.detail,
                t::CAPTION,
                Tone::secondary(Surface::Paper),
            );
            world.entity_mut(detail).insert(Part::Secondary);
            parts.detail = Some(detail);
            for slot in [&mut parts.chips, &mut parts.reason] {
                *slot = Some(
                    world
                        .spawn((
                            ChildOf(entity),
                            Node {
                                width: percent(100),
                                flex_wrap: FlexWrap::Wrap,
                                column_gap: px(5),
                                row_gap: px(4),
                                display: Display::None,
                                ..default()
                            },
                        ))
                        .id(),
                );
            }
            extras(world, entity, &mut parts, Surface::Paper);
            entity
        }
    };
    world.entity_mut(entity).insert((kind, parts));
    entity
}

fn update(world: &mut World, entity: Entity, row: &model::Row, index: usize) {
    let mut parts = world.get::<RowParts>(entity).cloned().unwrap_or_default();
    let kind = kind_of(row);
    if let Some(title) = parts.title {
        w::set_text(world, title, &row.title);
    }
    if let Some(detail) = parts.detail {
        // A slot shows the count alone: "12 in your pouch" is its tooltip.
        let text = if kind == RowKind::Item {
            row.detail.split(' ').next().unwrap_or_default().to_owned()
        } else {
            row.detail.clone()
        };
        w::set_text(world, detail, &text);
        if kind != RowKind::Item {
            w::show(world, detail, !row.detail.is_empty());
        }
    }
    if let Some(action) = &row.action {
        if world.get::<PlayerAction>(entity).map(|held| &held.0) != Some(action) {
            world
                .entity_mut(entity)
                .insert(PlayerAction(action.clone()));
        }
        w::set_enabled(world, entity, row.enabled);
        w::set_state(world, entity, row.selected, false);
        if world.get::<TabIndex>(entity).map(|tab| tab.0) != Some(100 + index as i32) {
            world
                .entity_mut(entity)
                .insert(TabIndex(100 + index as i32));
        }
        let name = if row.detail.is_empty() {
            row.title.clone()
        } else {
            format!("{}. {}", row.title, row.detail)
        };
        world
            .entity_mut(entity)
            .insert(bevy::ui::prelude::AccessibleLabel::new(name));
    }
    if kind == RowKind::Item {
        let tooltip = Tooltip::new(row.title.clone(), row.detail.clone())
            .detail(["Choose it to see what can be done with it."]);
        if world.get::<Tooltip>(entity) != Some(&tooltip) {
            world.entity_mut(entity).insert(tooltip);
        }
    }
    if matches!(kind, RowKind::Action | RowKind::Compact) {
        let tooltip = row_tip(row);
        if world.get::<Tooltip>(entity) != Some(&tooltip) {
            world.entity_mut(entity).insert(tooltip);
        }
    }
    if let Some(slot) = parts.picture {
        let good = item_kind(&row.key).unwrap_or_default().to_owned();
        if parts.picture_shown != good {
            world.entity_mut(slot).despawn_children();
            if let Some(picture) = Icon::of_good(&good) {
                let picture = icon::icon(world, slot, picture, 24.0, Tone::text(Surface::Paper));
                world.entity_mut(picture).insert(Part::Label);
            }
            parts.picture_shown = good;
        }
    }
    if let Some(slot) = parts.chips
        && parts.chips_shown != row.inputs
    {
        world.entity_mut(slot).despawn_children();
        for (name, owned, wanted) in &row.inputs {
            let chip = status_chip(
                world,
                slot,
                owned >= wanted,
                &format!("{name} {owned}/{wanted}"),
            );
            world.entity_mut(chip).insert((
                bevy::picking::hover::Hovered::default(),
                chip_tip(name, *owned, *wanted),
            ));
        }
        w::show(world, slot, !row.inputs.is_empty());
        parts.chips_shown.clone_from(&row.inputs);
    }
    if let Some(slot) = parts.reason {
        // The reason stands under a row that cannot be used; it goes when the row can.
        let reason = if row.enabled {
            String::new()
        } else {
            row.reason.clone()
        };
        if parts.reason_shown != reason {
            world.entity_mut(slot).despawn_children();
            if !reason.is_empty() {
                // The sentence takes the row's width and wraps inside it.
                let line = w::reason(world, slot, Surface::Paper, &reason);
                if let Some(mut node) = world.get_mut::<Node>(line) {
                    node.width = percent(100);
                    node.min_width = px(0);
                    node.flex_shrink = 1.0;
                }
            }
            w::show(world, slot, !reason.is_empty());
            parts.reason_shown = reason;
        }
    }
    if let Some(bar) = parts.meter {
        w::show(world, bar.entity, row.meter.is_some());
        if let Some((share, words)) = &row.meter {
            w::set_bar(world, bar, *share, Some(words));
        }
    }
    if let Some(pips) = parts.pips {
        w::show(world, pips, row.pips.is_some());
        if let Some((done, total)) = row.pips {
            w::set_pips(world, pips, Surface::Paper, done, None, total);
        }
    }
    if let Some(slot) = parts.buttons {
        // The card's buttons are built again when what they offer changes.
        let wanted: Vec<(String, bool, String)> = row
            .buttons
            .iter()
            .map(|button| (button.title.clone(), button.enabled, button.reason.clone()))
            .collect();
        if parts.buttons_shown != wanted {
            world.entity_mut(slot).despawn_children();
            for (at, offer) in row.buttons.iter().enumerate() {
                let line = world
                    .spawn((
                        ChildOf(slot),
                        Node {
                            align_items: AlignItems::Center,
                            column_gap: px(10),
                            ..default()
                        },
                    ))
                    .id();
                let button = w::button(
                    world,
                    line,
                    w::ButtonSpec::new(&offer.title, Kind::Secondary, Surface::Paper)
                        .reach(Reach::Tab(100 + index as i32 + at as i32)),
                    (PlayerControl, PlayerAction(offer.action.clone())),
                );
                if let Some(mut node) = world.get_mut::<Node>(button) {
                    node.min_width = px(132);
                    node.flex_shrink = 0.0;
                }
                w::set_enabled(world, button, offer.enabled);
                world
                    .entity_mut(button)
                    .insert(Tooltip::new(&offer.title, &offer.detail).detail(
                        if offer.reason.is_empty() {
                            vec!["It takes game time; Stop interrupts it.".to_owned()]
                        } else {
                            vec![offer.reason.clone()]
                        },
                    ));
                // What it does, or why it cannot be done now, beside it.
                let words = w::paragraph(
                    world,
                    line,
                    if offer.enabled || offer.reason.is_empty() {
                        offer.detail.clone()
                    } else {
                        offer.reason.clone()
                    },
                    t::CAPTION,
                    Tone::secondary(Surface::Paper),
                );
                if let Some(mut node) = world.get_mut::<Node>(words) {
                    node.width = Val::Auto;
                    node.flex_grow = 1.0;
                    node.flex_basis = px(0);
                }
            }
            w::show(world, slot, !row.buttons.is_empty());
            parts.buttons_shown = wanted;
        }
    }
    world.entity_mut(entity).insert(parts);
}

/// Keyed reconciliation of the workspace's rows: a row keeps its entity
/// (and its focus and scroll place) while its text changes.
pub(super) fn reconcile(world: &mut World, rows: Vec<model::Row>) {
    let Some(body) = world
        .query_filtered::<Entity, With<WorkspaceBody>>()
        .iter(world)
        .next()
    else {
        return;
    };
    let old = world.resource::<PlayerState>().rows.clone();
    let mut live = BTreeMap::new();
    let mut order = Vec::new();
    for (index, row) in rows.iter().enumerate() {
        let kept = old
            .get(&row.key)
            .copied()
            .filter(|entity| world.get::<RowKind>(*entity) == Some(&kind_of(row)));
        let entity = match kept {
            Some(entity) => entity,
            None => build(world, body, row, index),
        };
        update(world, entity, row, index);
        live.insert(row.key.clone(), entity);
        order.push(entity);
    }
    for (key, entity) in old {
        if live.get(&key) != Some(&entity) {
            if world.resource::<InputFocus>().get() == Some(entity) {
                world.resource_mut::<InputFocus>().clear();
            }
            if world.get_entity(entity).is_ok() {
                world.despawn(entity);
            }
        }
    }
    world.entity_mut(body).replace_children(&order);
    if std::mem::take(&mut world.resource_mut::<PlayerState>().focus_workspace) {
        let preferred = [
            "build_home",
            "claim_site",
            "plant",
            "water",
            "harvest",
            "greet",
            "open_garden",
        ]
        .into_iter()
        .filter_map(|key| live.get(key).copied())
        .find(|entity| world.get::<InteractionDisabled>(*entity).is_none());
        let bed = format!("bed_{}", world.resource::<PlayerState>().bed);
        let preferred = preferred.or_else(|| live.get(&bed).copied());
        let next = preferred.or_else(|| {
            order.iter().copied().find(|entity| {
                world.get::<PlayerAction>(*entity).is_some()
                    && world.get::<InteractionDisabled>(*entity).is_none()
            })
        });
        if let Some(entity) = next {
            world
                .resource_mut::<InputFocus>()
                .set(entity, FocusCause::Navigated);
            world.trigger(bevy::ui_widgets::ScrollIntoView { entity });
        }
    }
    world.resource_mut::<PlayerState>().rows = live;
}

/// The header, the width and the pouch pane for the open tab.
pub(super) fn frame(
    world: &mut World,
    tab: Tab,
    badge: Option<(Status, String)>,
    pouch: &[(String, String, u64)],
) {
    let layout = scale::layout(world);
    let (width, pane) = width(tab, layout.class);
    let top = crate::island_shell::content_top(world);
    for mut node in world
        .query_filtered::<&mut Node, With<WorkspaceRoot>>()
        .iter_mut(world)
    {
        if node.width != px(width) {
            node.width = px(width);
        }
        if node.top != px(top) {
            node.top = px(top);
        }
    }
    for mut text in world
        .query_filtered::<&mut Text, With<WorkspaceTitle>>()
        .iter_mut(world)
    {
        if text.0 != tab.title() {
            text.0 = tab.title().into();
        }
    }
    let shown = world.resource::<Frame>().clone();
    if shown.tab != Some(tab) {
        let slots: Vec<Entity> = world
            .query_filtered::<Entity, With<WorkspaceIcon>>()
            .iter(world)
            .collect();
        for slot in slots {
            world.entity_mut(slot).despawn_children();
            icon::icon(world, slot, tab.icon(), 20.0, Tone::text(Surface::Paper));
        }
        // What the workspace is for is its title's tooltip, not a line of text.
        let titles: Vec<Entity> = world
            .query_filtered::<Entity, With<WorkspaceTitle>>()
            .iter(world)
            .collect();
        for title in titles {
            world.entity_mut(title).insert((
                Hovered::default(),
                Tooltip::new(tab.title(), tab.subtitle()).key(tab.key()),
            ));
        }
    }
    if shown.badge != badge {
        let slots: Vec<Entity> = world
            .query_filtered::<Entity, With<WorkspaceBadge>>()
            .iter(world)
            .collect();
        for slot in slots {
            world.entity_mut(slot).despawn_children();
            if let Some((status, word)) = &badge {
                w::badge(world, slot, Surface::Paper, *status, word);
            }
        }
    }
    // The pouch pane: what you carry, as slots. It only shows; the Pouch
    // workspace is where things are used. Empty, it is one line above the
    // list, which then takes the whole width.
    let layout = pouch_layout(pane, pouch.is_empty());
    let panes: Vec<Entity> = world
        .query_filtered::<Entity, With<PouchPane>>()
        .iter(world)
        .collect();
    for entity in panes {
        let content = world.get::<ChildOf>(entity).map(ChildOf::parent);
        if let Some(mut node) = world.get_mut::<Node>(entity) {
            let (display, width, direction, border, padding) = match layout {
                PouchLayout::Hidden => (
                    Display::None,
                    node.width,
                    node.flex_direction,
                    node.border,
                    node.padding,
                ),
                PouchLayout::Line => (
                    Display::Flex,
                    percent(100),
                    FlexDirection::Row,
                    UiRect::bottom(px(1)),
                    UiRect::bottom(px(8)),
                ),
                PouchLayout::Pane(width) => (
                    Display::Flex,
                    px(width),
                    FlexDirection::Column,
                    UiRect::right(px(1)),
                    UiRect::right(px(10)),
                ),
            };
            if node.display != display {
                node.display = display;
            }
            if node.width != width {
                node.width = width;
            }
            if node.flex_direction != direction {
                node.flex_direction = direction;
                node.align_items = if direction == FlexDirection::Row {
                    AlignItems::Center
                } else {
                    AlignItems::Stretch
                };
                node.column_gap = px(8);
            }
            if node.border != border {
                node.border = border;
            }
            if node.padding != padding {
                node.padding = padding;
            }
        }
        // The pane and the list share a row; the line stands above the list.
        let direction = if layout == PouchLayout::Line {
            FlexDirection::Column
        } else {
            FlexDirection::Row
        };
        if let Some(content) = content
            && let Some(mut node) = world.get_mut::<Node>(content)
            && node.flex_direction != direction
        {
            node.flex_direction = direction;
            node.row_gap = px(8);
        }
    }
    let carried: Vec<(String, String, u64)> = if pane.is_some() {
        pouch.to_vec()
    } else {
        Vec::new()
    };
    if shown.pouch.as_ref() != Some(&carried) {
        let holders: Vec<Entity> = world
            .query_filtered::<Entity, With<PouchSlots>>()
            .iter(world)
            .collect();
        for holder in holders {
            world.entity_mut(holder).despawn_children();
            if carried.is_empty() {
                w::label(
                    world,
                    holder,
                    POUCH_EMPTY,
                    t::CAPTION,
                    Tone::secondary(Surface::Paper),
                );
            }
            for (kind, name, count) in &carried {
                let slot = w::card(world, holder, 4.0);
                {
                    let mut node = world.get_mut::<Node>(slot).expect("slot");
                    node.width = px(74);
                    node.min_height = px(58);
                    node.align_items = AlignItems::Center;
                    node.flex_direction = FlexDirection::Column;
                    node.row_gap = px(1);
                }
                let top = world
                    .spawn((
                        ChildOf(slot),
                        Node {
                            align_items: AlignItems::Center,
                            column_gap: px(4),
                            height: px(26),
                            ..default()
                        },
                    ))
                    .id();
                if let Some(picture) = Icon::of_good(kind) {
                    icon::icon(world, top, picture, 20.0, Tone::text(Surface::Paper));
                }
                w::label(
                    world,
                    top,
                    count.to_string(),
                    t::BODY,
                    Tone::text(Surface::Paper),
                );
                w::label(
                    world,
                    slot,
                    name,
                    t::CAPTION,
                    Tone::secondary(Surface::Paper),
                );
            }
        }
    }
    *world.resource_mut::<Frame>() = Frame {
        tab: Some(tab),
        badge,
        pouch: Some(carried),
    };
}

/// The empty pouch pane's one line.
pub(super) const POUCH_EMPTY: &str = "Empty: what you gather, make or buy shows here.";

/// How a workspace shows the pouch: not at all (one-pane tabs), as one line
/// above the list when it is empty, or as a pane of the given width.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum PouchLayout {
    Hidden,
    Line,
    Pane(f32),
}

pub(super) fn pouch_layout(pane: Option<f32>, empty: bool) -> PouchLayout {
    match pane {
        None => PouchLayout::Hidden,
        Some(_) if empty => PouchLayout::Line,
        Some(width) => PouchLayout::Pane(width),
    }
}

/// What the workspace's frame last showed.
#[derive(Resource, Clone, Default)]
pub(super) struct Frame {
    tab: Option<Tab>,
    badge: Option<(Status, String)>,
    /// `None` until the pane was filled once: an empty pouch says so.
    pouch: Option<Vec<(String, String, u64)>>,
}
