//! Rows of the right column, built from the shared widget kit, and keyed row
//! reconciliation. The ledger's pages and the resident inspector's tabs are
//! both lists of [`Entry`]. Presentation only.
use super::economy::Trend;
use super::model::{Entry, Extra};
use super::{Action, Ledger, LedgerAction, LedgerBody, LedgerControl, LedgerFooter};
use crate::game_ui::icon::{self, Icon};
use crate::game_ui::tokens::{self as t, Status, Surface};
use crate::game_ui::tooltip::Tooltip;
use crate::game_ui::widgets::{self as w, ButtonSpec, Kind, Part, Reach, Role, Size, Swatch, Tone};
use crate::resident_inspector::model::NeedLevel;
use bevy::input_focus::InputFocus;
use bevy::input_focus::tab_navigation::TabIndex;
use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use std::collections::BTreeMap;

/// Column widths of a ledger table, in percent of the row. The narrow
/// column gives the two numbers more of it, so a sorted header keeps its
/// arrow, and its last header is one short word.
pub(super) fn column_widths(compact: bool) -> [f32; 4] {
    if compact {
        [38.0, 18.0, 18.0, 26.0]
    } else {
        [40.0, 14.0, 14.0, 32.0]
    }
}

/// The widths of a table of `count` columns: the goods table has a fifth,
/// the days its stock lasts, between the price and the change.
pub(super) fn widths_for(count: usize, compact: bool) -> Vec<f32> {
    match (count, compact) {
        (5, true) => vec![28.0, 16.0, 16.0, 14.0, 26.0],
        (5, false) => vec![32.0, 13.0, 13.0, 12.0, 30.0],
        _ => column_widths(compact).to_vec(),
    }
}

fn compact(world: &World) -> bool {
    crate::game_ui::scale::layout(world).class == crate::game_ui::scale::LayoutClass::Compact
}

/// What kind of widget a keyed row is. A key keeps its kind: a table row
/// never becomes text.
#[derive(Component, Clone, Debug, PartialEq, Eq)]
enum RowKind {
    Text {
        heading: bool,
    },
    Action,
    Table {
        heading: bool,
        columns: usize,
        interactive: bool,
        badge: Option<String>,
        /// A sortable header: its sorted column and its words.
        sort: Option<(Option<(u8, bool)>, String)>,
    },
    /// Rebuilt when its content changes.
    Built(String),
    /// A two-line row with a status shape: a resident or a journal line.
    Marked {
        lines: usize,
    },
}

/// The parts of a row a refresh rewrites in place.
#[derive(Component, Clone, Debug, Default)]
struct RowParts {
    texts: Vec<Entity>,
    /// A cell's smaller second word, by column: the price's change.
    subs: Vec<Option<Entity>>,
    /// The cells of a table row, in column order.
    cells: Vec<Entity>,
    /// The node that holds a status shape or a trend arrow.
    mark: Option<Entity>,
}

/// The shape a mark slot currently shows.
#[derive(Component, Clone, Copy, PartialEq)]
struct Marked(Icon, Status);

pub(crate) fn need_status(level: NeedLevel) -> Status {
    match level {
        NeedLevel::Fine => Status::Good,
        NeedLevel::Low => Status::Warn,
        NeedLevel::Critical => Status::Crit,
    }
}

fn trend_mark(trend: Trend) -> (Icon, Status) {
    match trend {
        Trend::Rising(_) => (Icon::TrendUp, Status::Good),
        Trend::Falling(_) => (Icon::TrendDown, Status::Crit),
        Trend::Steady | Trend::Unknown => (Icon::Unchanged, Status::Idle),
    }
}

fn set_mark(world: &mut World, slot: Entity, wanted: (Icon, Status)) {
    let wanted = Marked(wanted.0, wanted.1);
    if world.get::<Marked>(slot) == Some(&wanted) {
        return;
    }
    world.entity_mut(slot).despawn_children();
    icon::icon(
        world,
        slot,
        wanted.0,
        if matches!(wanted.0, Icon::TrendUp | Icon::TrendDown | Icon::Unchanged) {
            12.0
        } else {
            16.0
        },
        Tone::status(Surface::Paper, wanted.1),
    );
    world.entity_mut(slot).insert(wanted);
}

fn mark_slot(world: &mut World, parent: Entity, size: f32) -> Entity {
    world
        .spawn((
            ChildOf(parent),
            Node {
                width: px(size),
                height: px(size),
                flex_shrink: 0.0,
                ..default()
            },
        ))
        .id()
}

pub(super) fn row(world: &mut World, parent: Entity) -> Entity {
    world
        .spawn((
            ChildOf(parent),
            Node {
                flex_wrap: FlexWrap::Wrap,
                column_gap: px(6),
                row_gap: px(6),
                flex_shrink: 0.0,
                ..default()
            },
        ))
        .id()
}

/// A ledger button in the developer header rows.
pub(super) fn button(world: &mut World, parent: Entity, label: &str, action: Action) -> Entity {
    w::button(
        world,
        parent,
        ButtonSpec::new(label, Kind::Secondary, Surface::Paper).size(Size::Small),
        (LedgerControl, LedgerAction(action)),
    )
}

pub(super) fn set_text<M: Component>(world: &mut World, value: &str) {
    for mut text in world.query_filtered::<&mut Text, With<M>>().iter_mut(world) {
        if text.0 != value {
            text.0 = value.into();
        }
    }
}

fn wide(world: &mut World, text: Entity) {
    // A definite width, not a zero flex basis: a text node measured at zero
    // width reports that measure as its content size.
    world.entity_mut(text).insert(Node {
        width: percent(100),
        flex_shrink: 1.0,
        min_width: px(0),
        ..default()
    });
}

/// A full-width action: a sentence that does something when chosen. A link
/// (`Extra::Link`) is quiet and ends in an arrow.
fn action_row(world: &mut World, parent: Entity, entry: &Entry, action: Action) -> Entity {
    let link = entry.extra == Some(Extra::Link);
    let entity = w::control(
        world,
        parent,
        &entry.text,
        if link { Kind::Row } else { Kind::Secondary },
        Surface::Paper,
        Reach::Tab(0),
        Node {
            min_height: px(if link {
                t::ROW_ONE_LINE
            } else {
                t::BUTTON_MEDIUM
            }),
            padding: UiRect::axes(px(if link { 7 } else { 10 }), px(if link { 2 } else { 6 })),
            column_gap: px(8),
            border: if link {
                UiRect::left(px(3))
            } else {
                UiRect::all(px(1))
            },
            border_radius: BorderRadius::all(px(t::RADIUS_MD)),
            align_items: AlignItems::Center,
            flex_shrink: 0.0,
            ..default()
        },
        (LedgerControl, LedgerAction(action)),
    );
    if let Some(picture) = entry.icon {
        let picture = icon::icon(
            world,
            entity,
            picture,
            16.0,
            Tone::secondary(Surface::Paper),
        );
        world.entity_mut(picture).insert(Part::Secondary);
    }
    let text = w::paragraph(
        world,
        entity,
        &entry.text,
        t::DENSE,
        Tone::text(Surface::Paper),
    );
    world.entity_mut(text).insert(Part::Label);
    wide(world, text);
    if link {
        icon::icon(
            world,
            entity,
            Icon::GoTo,
            12.0,
            Tone::caption(Surface::Paper),
        );
    }
    world.entity_mut(entity).insert((
        RowKind::Action,
        RowParts {
            texts: vec![text],
            mark: None,
            ..default()
        },
    ));
    entity
}

/// A two-line row with a status shape in front: a resident of the roster or
/// a line of the journal. Reached with Tab, never focused by a click: choosing
/// one hands the view to the world.
fn marked_row(world: &mut World, parent: Entity, entry: &Entry, action: Action) -> Entity {
    let lines: Vec<&str> = entry.text.split('\n').collect();
    let entity = w::list_row(
        world,
        parent,
        &entry.text,
        lines.len() > 1,
        Reach::Ring,
        (LedgerControl, LedgerAction(action)),
    );
    let mark = mark_slot(world, entity, 16.0);
    let column = world
        .spawn((
            ChildOf(entity),
            Node {
                flex_direction: FlexDirection::Column,
                flex_grow: 1.0,
                flex_shrink: 1.0,
                min_width: px(0),
                overflow: Overflow::clip_x(),
                ..default()
            },
        ))
        .id();
    let mut texts = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        let text = if index == 0 {
            let text = w::paragraph(world, column, *line, t::DENSE, Tone::text(Surface::Paper));
            world.entity_mut(text).insert(Part::Label);
            text
        } else {
            let text = w::paragraph(
                world,
                column,
                *line,
                t::CAPTION,
                Tone::secondary(Surface::Paper),
            );
            world.entity_mut(text).insert(Part::Secondary);
            text
        };
        texts.push(text);
    }
    icon::icon(
        world,
        entity,
        Icon::GoTo,
        12.0,
        Tone::caption(Surface::Paper),
    );
    world.entity_mut(entity).insert((
        RowKind::Marked { lines: lines.len() },
        RowParts {
            texts,
            mark: Some(mark),
            ..default()
        },
    ));
    entity
}

fn badge_of(entry: &Entry) -> Option<(Status, &str)> {
    match &entry.extra {
        Some(Extra::Badge(status, word)) => Some((*status, word.as_str())),
        _ => None,
    }
}

fn table_kind(entry: &Entry) -> RowKind {
    RowKind::Table {
        heading: entry.heading,
        columns: entry.columns.as_ref().map_or(0, Vec::len),
        interactive: entry.action.is_some(),
        badge: badge_of(entry).map(|(_, word)| word.to_owned()),
        // A header is built again when its sorted column changes, and when
        // its words do (the narrow column says "Change").
        sort: match &entry.extra {
            Some(Extra::SortHeader(sort)) => Some((
                *sort,
                entry.columns.as_deref().unwrap_or_default().join("\u{1f}"),
            )),
            _ => None,
        },
    }
}

/// A table row. With an action it is a selectable control; the header and
/// read-only rows are plain nodes.
fn table_row(world: &mut World, parent: Entity, entry: &Entry) -> Entity {
    let columns = entry.columns.as_deref().unwrap_or_default();
    let row = match &entry.action {
        Some(action) => w::table_row(
            world,
            parent,
            &entry.text,
            Reach::Tab(0),
            (LedgerControl, LedgerAction(action.clone())),
        ),
        None => world
            .spawn((
                ChildOf(parent),
                Swatch::Edge,
                Tone::new(
                    Surface::Paper,
                    if entry.heading {
                        Role::Control
                    } else {
                        Role::Edge
                    },
                ),
                Node {
                    width: percent(100),
                    height: px(if entry.heading {
                        t::ROW_ONE_LINE
                    } else {
                        t::TABLE_ROW
                    }),
                    align_items: AlignItems::Center,
                    flex_shrink: 0.0,
                    padding: UiRect::horizontal(px(6)),
                    border: UiRect::bottom(px(1)),
                    ..default()
                },
                BorderColor::all(Color::NONE),
            ))
            .id(),
    };
    let mut parts = RowParts::default();
    let widths = widths_for(columns.len(), compact(world));
    let last = columns.len().saturating_sub(1);
    for (index, value) in columns.iter().enumerate() {
        let cell = w::table_cell(
            world,
            row,
            widths.get(index).copied().unwrap_or(20.0),
            index == 0,
        );
        parts.cells.push(cell);
        if index == 0
            && !entry.heading
            && let Some(picture) = entry.icon
        {
            // The good's own icon, where one is mapped.
            icon::icon(world, cell, picture, 16.0, Tone::secondary(Surface::Paper));
        }
        if index == last && entry.trend.is_some() {
            parts.mark = Some(mark_slot(world, cell, 12.0));
        }
        let tone = if entry.heading {
            Tone::secondary(Surface::Paper)
        } else {
            Tone::text(Surface::Paper)
        };
        if let Some(Extra::SortHeader(sorted)) = &entry.extra {
            // A header that sorts: a quiet control with the column's name
            // and, on the sorted column, the arrow of its direction.
            let button = w::control(
                world,
                cell,
                &format!("Sort by {}", value.to_lowercase()),
                Kind::Quiet,
                Surface::Paper,
                Reach::Tab(0),
                // No padding of its own: the name stays where a plain header
                // has it, and the arrow fits inside the narrow columns.
                Node {
                    height: px(22),
                    column_gap: px(2),
                    align_items: AlignItems::Center,
                    border_radius: BorderRadius::all(px(t::RADIUS_SM)),
                    flex_shrink: 0.0,
                    ..default()
                },
                (LedgerControl, LedgerAction(Action::SortGoods(index as u8))),
            );
            let text = w::label(world, button, value, t::DENSE, tone);
            world.entity_mut(text).insert(Part::Secondary);
            if let Some((_, ascending)) = sorted.filter(|(column, _)| usize::from(*column) == index)
            {
                let arrow = icon::icon(
                    world,
                    button,
                    if ascending {
                        Icon::SortUp
                    } else {
                        Icon::SortDown
                    },
                    9.0,
                    Tone::secondary(Surface::Paper),
                );
                world.entity_mut(arrow).insert(Part::Secondary);
            }
            let mut tip = Tooltip::new(
                format!("Sort by {}", value.to_lowercase()),
                "Order the goods of each group by this column. Choose it again to reverse.",
            );
            if let Some(meaning) = header_tip(value) {
                tip = tip.detail([meaning]);
            }
            world.entity_mut(button).insert(tip);
            parts.texts.push(text);
            continue;
        }
        // The goods table's price: the price in the row's type, its change
        // since yesterday smaller and grey beside it, taking no side (2.2).
        let priced = !entry.heading && entry.trend.is_some() && index == 2;
        let (main, change) = if priced {
            split_change(value)
        } else {
            (value.as_str(), None)
        };
        let text = w::label(world, cell, main, t::DENSE, tone);
        parts.subs.resize(index, None);
        if priced {
            let sub = w::label(
                world,
                cell,
                change.unwrap_or(""),
                t::CAPTION,
                Tone::secondary(Surface::Paper),
            );
            if let Some(mut node) = world.get_mut::<Node>(sub) {
                node.margin = UiRect::left(px(3));
            }
            parts.subs.push(Some(sub));
        } else {
            parts.subs.push(None);
        }
        // A plain header says what its column counts.
        if entry.heading
            && let Some(meaning) = header_tip(value)
        {
            world.entity_mut(cell).insert((
                bevy::picking::hover::Hovered::default(),
                Tooltip::new(value.clone(), meaning),
            ));
        }
        if entry.action.is_some() {
            world.entity_mut(text).insert(Part::Label);
        }
        parts.texts.push(text);
        if index == 0
            && let Some((status, word)) = badge_of(entry)
        {
            w::badge(world, cell, Surface::Paper, status, word);
        }
    }
    world.entity_mut(row).insert((table_kind(entry), parts));
    row
}

/// "4 +1" as the price and its change; "4" and "–" alone.
pub(super) fn split_change(value: &str) -> (&str, Option<&str>) {
    match value.split_once(' ') {
        Some((price, change))
            if change.len() > 1
                && change.starts_with(['+', '-'])
                && change[1..].chars().all(|c| c.is_ascii_digit()) =>
        {
            (price, Some(change))
        }
        _ => (value, None),
    }
}

/// What a table header's column counts, for its tooltip.
pub(super) fn header_tip(header: &str) -> Option<&'static str> {
    Some(match header {
        "Good" => "Every good on the island, in three groups.",
        "Stock" => "Units held in packs, stores and stalls.",
        "Price" => "What one costs at the market, in coins, and the change since yesterday.",
        "Days" => "Days the stock lasts at about one day's use.",
        "Since yesterday" | "Change" => {
            "How much the stock rose or fell since this time yesterday."
        }
        "Job" => "Each job in the village.",
        "Workers" => "Residents working in it now.",
        "Open" => "Workplaces with a free place.",
        "Now" => "Residents of this village who do this job now.",
        "Wanted" => {
            "How many you want working it. A dash: the village decides. Residents change job one at a time, after the day's work."
        }
        _ => return None,
    })
}

/// Whether a set of choices selects one of several (a segmented control)
/// rather than doing things (buttons).
fn selector(choices: &[(String, Action)]) -> bool {
    choices.len() > 1
        && choices.iter().all(|(_, action)| {
            matches!(
                action,
                Action::Sort(_)
                    | Action::Scope(_)
                    | Action::AlertFilter(_)
                    | Action::Reserve(_)
                    | Action::EconomyPage(_)
                    | Action::Priority { .. }
            )
        })
}

fn choice_row(world: &mut World, parent: Entity, entry: &Entry) -> Entity {
    let choices = entry.choices.as_deref().unwrap_or_default();
    let entity = row(world, parent);
    if selector(choices) {
        let group = w::segmented(world, entity, Surface::Paper);
        world.get_mut::<Node>(group).expect("segments").flex_grow = 1.0;
        for (label, action) in choices {
            let segment = w::segment(
                world,
                group,
                label,
                None,
                Some(label),
                None,
                Reach::Tab(0),
                (LedgerControl, LedgerAction(action.clone())),
            );
            let mut node = world.get_mut::<Node>(segment).expect("segment");
            // Share the row evenly; four village names fit the 400 px column.
            node.flex_grow = 1.0;
            node.padding = UiRect::horizontal(px(5));
            node.min_width = px(0);
            if let Some(tip) = super::economy::choice_tip(action) {
                world.entity_mut(segment).insert(tip);
            }
        }
    } else {
        for (label, action) in choices {
            let kind = if matches!(action, Action::ConfirmCancelSite(_)) {
                Kind::Danger
            } else {
                Kind::Secondary
            };
            let button = w::button(
                world,
                entity,
                ButtonSpec::new(label, kind, Surface::Paper).size(Size::Small),
                (LedgerControl, LedgerAction(action.clone())),
            );
            if let Some(tip) = super::economy::choice_tip(action) {
                world.entity_mut(button).insert(tip);
            }
        }
    }
    entity
}

/// Stat chips in a two-column grid: label over value, in a soft box.
fn stat_grid(world: &mut World, parent: Entity, entry: &Entry) -> Entity {
    let entity = row(world, parent);
    for (picture, label, value) in entry.stats.as_deref().unwrap_or_default() {
        let chip = world
            .spawn((
                ChildOf(entity),
                w::Skin::Card,
                Node {
                    width: percent(48.5),
                    height: px(40),
                    padding: UiRect::horizontal(px(8)),
                    column_gap: px(8),
                    border: UiRect::all(px(1)),
                    border_radius: BorderRadius::all(px(t::RADIUS_MD)),
                    align_items: AlignItems::Center,
                    ..default()
                },
                BackgroundColor(Color::NONE),
                BorderColor::all(Color::NONE),
            ))
            .id();
        icon::icon(world, chip, *picture, 20.0, Tone::secondary(Surface::Paper));
        let column = world
            .spawn((
                ChildOf(chip),
                Node {
                    flex_direction: FlexDirection::Column,
                    ..default()
                },
            ))
            .id();
        w::label(
            world,
            column,
            label,
            t::CAPTION,
            Tone::caption(Surface::Paper),
        );
        w::label(world, column, value, t::BODY, Tone::text(Surface::Paper));
    }
    entity
}

fn chip(world: &mut World, parent: Entity, picture: Option<Icon>, text: &str) {
    let chip = world
        .spawn((
            ChildOf(parent),
            w::Skin::Card,
            Node {
                height: px(t::CHIP_PAPER),
                padding: UiRect::horizontal(px(8)),
                column_gap: px(5),
                border: UiRect::all(px(1)),
                border_radius: BorderRadius::all(px(t::RADIUS_MD)),
                align_items: AlignItems::Center,
                flex_shrink: 0.0,
                ..default()
            },
            BackgroundColor(Color::NONE),
            BorderColor::all(Color::NONE),
        ))
        .id();
    if let Some(picture) = picture {
        icon::icon(world, chip, picture, 14.0, Tone::secondary(Surface::Paper));
    }
    w::label(world, chip, text, t::DENSE, Tone::text(Surface::Paper));
}

/// The rows that are built once and rebuilt when their content changes.
fn built(world: &mut World, body: Entity, entry: &Entry, extra: &Extra) -> Entity {
    match extra {
        Extra::Meter {
            percent,
            status,
            value,
        } => {
            let need = w::need_bar(
                world,
                body,
                Surface::Paper,
                entry.icon.unwrap_or(Icon::Fine),
                &entry.text,
                f32::from(*percent) / 100.0,
                *status,
            );
            let text = w::label(
                world,
                need.bar.entity,
                value,
                t::DENSE,
                Tone::secondary(Surface::Paper),
            );
            world.entity_mut(text).insert(Node {
                width: px(34),
                justify_content: JustifyContent::FlexEnd,
                flex_shrink: 0.0,
                ..default()
            });
            need.bar.entity
        }
        Extra::Chips(chips) => {
            let entity = row(world, body);
            for (picture, text) in chips {
                chip(world, entity, *picture, text);
            }
            entity
        }
        Extra::Card(lines) => {
            let entity = w::card(world, body, 10.0);
            let head = world
                .spawn((
                    ChildOf(entity),
                    Node {
                        column_gap: px(6),
                        align_items: AlignItems::Center,
                        ..default()
                    },
                ))
                .id();
            if let Some(picture) = entry.icon {
                icon::icon(world, head, picture, 16.0, Tone::text(Surface::Paper));
            }
            w::label(
                world,
                head,
                &entry.text,
                t::BODY,
                Tone::text(Surface::Paper),
            );
            for (index, line) in lines.iter().enumerate() {
                let text = w::paragraph(
                    world,
                    entity,
                    line,
                    t::DENSE,
                    Tone::secondary(Surface::Paper),
                );
                // A line that says more on hover (the last market, what
                // waits since when).
                if let Some((_, tip)) = entry.line_tips.iter().find(|(at, _)| *at == index) {
                    world
                        .entity_mut(text)
                        .insert((Hovered::default(), tip.clone()));
                }
            }
            entity
        }
        Extra::Group => w::table_group(world, body, &entry.text),
        Extra::Fold(open) => {
            let entity = w::control(
                world,
                body,
                &entry.text,
                Kind::Quiet,
                Surface::Paper,
                Reach::Tab(0),
                Node {
                    min_height: px(t::ROW_ONE_LINE),
                    padding: UiRect::horizontal(px(4)),
                    column_gap: px(6),
                    border_radius: BorderRadius::all(px(t::RADIUS_SM)),
                    align_items: AlignItems::Center,
                    flex_shrink: 0.0,
                    ..default()
                },
                (
                    LedgerControl,
                    LedgerAction(entry.action.clone().expect("a fold has an action")),
                ),
            );
            icon::icon(
                world,
                entity,
                if *open {
                    Icon::Expanded
                } else {
                    Icon::Collapsed
                },
                12.0,
                Tone::secondary(Surface::Paper),
            );
            w::label(
                world,
                entity,
                &entry.text,
                t::SUBHEADING,
                Tone::text(Surface::Paper),
            );
            entity
        }
        Extra::Strip(segments, marker) => {
            let entity = world
                .spawn((
                    ChildOf(body),
                    Node {
                        height: px(14),
                        flex_shrink: 0.0,
                        ..default()
                    },
                ))
                .id();
            let strip = world
                .spawn((
                    ChildOf(entity),
                    Swatch::Edge,
                    Tone::new(Surface::Paper, Role::Control),
                    Node {
                        position_type: PositionType::Absolute,
                        left: px(0),
                        right: px(0),
                        top: px(2),
                        height: px(10),
                        border: UiRect::all(px(1)),
                        border_radius: BorderRadius::all(px(t::RADIUS_SM)),
                        overflow: Overflow::clip(),
                        ..default()
                    },
                    BorderColor::all(Color::NONE),
                ))
                .id();
            for (share, [r, g, b]) in segments {
                world.spawn((
                    ChildOf(strip),
                    Node {
                        width: percent(*share),
                        height: percent(100),
                        ..default()
                    },
                    BackgroundColor(Color::srgb(*r, *g, *b)),
                ));
            }
            // "Now": a mark in ink across the strip.
            world.spawn((
                ChildOf(entity),
                Swatch::Fill,
                Tone::text(Surface::Paper),
                Node {
                    position_type: PositionType::Absolute,
                    left: percent(*marker),
                    top: px(0),
                    width: px(3),
                    height: px(14),
                    border_radius: BorderRadius::all(px(1)),
                    ..default()
                },
                BackgroundColor(Color::NONE),
            ));
            entity
        }
        Extra::Site {
            percent,
            materials,
            status,
        } => {
            let entity = w::card_button(
                world,
                body,
                &entry.text,
                10.0,
                Reach::Tab(0),
                (
                    LedgerControl,
                    LedgerAction(entry.action.clone().expect("a site has an action")),
                ),
            );
            let head = world
                .spawn((
                    ChildOf(entity),
                    Node {
                        column_gap: px(6),
                        align_items: AlignItems::Center,
                        ..default()
                    },
                ))
                .id();
            let picture = icon::icon(
                world,
                head,
                Icon::BuildingSite,
                20.0,
                Tone::text(Surface::Paper),
            );
            world.entity_mut(picture).insert(Part::Label);
            let title = entry.text.split(" \u{b7} ").next().unwrap_or(&entry.text);
            let text = w::label(world, head, title, t::BODY, Tone::text(Surface::Paper));
            world.entity_mut(text).insert(Part::Label);
            w::progress_bar(
                world,
                entity,
                Surface::Paper,
                f32::from(*percent) / 100.0,
                Some(&format!("{percent}%")),
            );
            let chips = row(world, entity);
            for (picture, text) in materials {
                chip(world, chips, *picture, text);
            }
            if !status.is_empty() {
                let text = w::paragraph(
                    world,
                    entity,
                    status,
                    t::CAPTION,
                    Tone::secondary(Surface::Paper),
                );
                world.entity_mut(text).insert(Part::Secondary);
            }
            entity
        }
        Extra::Spark(heights) => {
            let entity = world
                .spawn((
                    ChildOf(body),
                    Node {
                        column_gap: px(8),
                        align_items: AlignItems::Center,
                        flex_shrink: 0.0,
                        ..default()
                    },
                ))
                .id();
            // Decoration beside the words that carry the same fact: never a
            // control, never picked.
            let bars = world
                .spawn((
                    ChildOf(entity),
                    bevy::picking::Pickable::IGNORE,
                    Node {
                        height: px(16),
                        column_gap: px(2),
                        align_items: AlignItems::FlexEnd,
                        flex_shrink: 0.0,
                        ..default()
                    },
                ))
                .id();
            for (index, height) in heights.iter().enumerate() {
                let last = index + 1 == heights.len();
                world.spawn((
                    ChildOf(bars),
                    bevy::picking::Pickable::IGNORE,
                    Swatch::Fill,
                    if last {
                        Tone::text(Surface::Paper)
                    } else {
                        Tone::secondary(Surface::Paper)
                    },
                    Node {
                        width: px(4),
                        height: px(*height as f32),
                        ..default()
                    },
                    BackgroundColor(Color::NONE),
                ));
            }
            w::label(
                world,
                entity,
                &entry.text,
                t::DENSE,
                Tone::secondary(Surface::Paper),
            );
            entity
        }
        Extra::Order(status, button_spec) => {
            use super::orders::OrderStatus;
            let entity = world
                .spawn((
                    ChildOf(body),
                    Node {
                        column_gap: px(6),
                        align_items: AlignItems::Center,
                        flex_shrink: 0.0,
                        ..default()
                    },
                ))
                .id();
            let (picture, shade) = match status {
                OrderStatus::Queued => (Icon::Pause, Status::Idle),
                OrderStatus::Working | OrderStatus::Priority => {
                    (Icon::status(Status::Info), Status::Info)
                }
                OrderStatus::Met => (Icon::status(Status::Good), Status::Good),
                OrderStatus::Refused => (Icon::status(Status::Crit), Status::Crit),
            };
            icon::icon(
                world,
                entity,
                picture,
                14.0,
                Tone::status(Surface::Paper, shade),
            );
            let text = w::paragraph(
                world,
                entity,
                &entry.text,
                t::DENSE,
                Tone::text(Surface::Paper),
            );
            if let Some(mut node) = world.get_mut::<Node>(text) {
                node.flex_grow = 1.0;
                node.flex_shrink = 1.0;
                node.min_width = px(0);
            }
            if let Some((label, action)) = button_spec {
                button(world, entity, label, action.clone());
            }
            entity
        }
        Extra::Strong => w::paragraph(
            world,
            body,
            &entry.text,
            t::DENSE,
            Tone::text(Surface::Paper),
        ),
        Extra::Caption => w::paragraph(
            world,
            body,
            &entry.text,
            t::CAPTION,
            Tone::caption(Surface::Paper),
        ),
        Extra::Link | Extra::Badge(..) | Extra::SortHeader(_) => {
            unreachable!("built as an action or a table row")
        }
    }
}

fn signature(entry: &Entry) -> Option<String> {
    if let Some(choices) = &entry.choices {
        // Built once per set of options; selection is painted, not rebuilt.
        return Some(
            choices
                .iter()
                .map(|(label, _)| label.as_str())
                .collect::<Vec<_>>()
                .join("\u{1f}"),
        );
    }
    if let Some(stats) = &entry.stats {
        return Some(
            stats
                .iter()
                .map(|(_, label, value)| format!("{label}={value}"))
                .collect::<Vec<_>>()
                .join("\u{1f}"),
        );
    }
    match &entry.extra {
        None | Some(Extra::Link | Extra::Badge(..) | Extra::SortHeader(_)) => None,
        Some(extra) => Some(format!("{extra:?}\u{1f}{}", entry.text)),
    }
}

fn kind_of(entry: &Entry) -> RowKind {
    if let Some(signature) = signature(entry) {
        RowKind::Built(signature)
    } else if entry.columns.is_some() {
        table_kind(entry)
    } else if entry.action.is_some() && (entry.indicator.is_some() || entry.shape.is_some()) {
        RowKind::Marked {
            lines: entry.text.split('\n').count(),
        }
    } else if entry.action.is_some() {
        RowKind::Action
    } else {
        RowKind::Text {
            heading: entry.heading,
        }
    }
}

fn build(world: &mut World, body: Entity, entry: &Entry) -> Entity {
    if entry.choices.is_some() {
        let entity = choice_row(world, body, entry);
        world.entity_mut(entity).insert(kind_of(entry));
        entity
    } else if entry.stats.is_some() {
        let entity = stat_grid(world, body, entry);
        world.entity_mut(entity).insert(kind_of(entry));
        entity
    } else if let Some(extra) = entry
        .extra
        .as_ref()
        .filter(|extra| !matches!(extra, Extra::Link | Extra::Badge(..) | Extra::SortHeader(_)))
    {
        let entity = built(world, body, entry, extra);
        world.entity_mut(entity).insert(kind_of(entry));
        entity
    } else if entry.columns.is_some() {
        table_row(world, body, entry)
    } else if let Some(action) = &entry.action {
        if entry.indicator.is_some() || entry.shape.is_some() {
            marked_row(world, body, entry, action.clone())
        } else {
            action_row(world, body, entry, action.clone())
        }
    } else {
        let entity = if entry.heading {
            w::label(
                world,
                body,
                &entry.text,
                t::SUBHEADING,
                Tone::text(Surface::Paper),
            )
        } else {
            w::paragraph(
                world,
                body,
                &entry.text,
                t::DENSE,
                Tone::secondary(Surface::Paper),
            )
        };
        world.entity_mut(entity).insert(kind_of(entry));
        entity
    }
}

fn update(world: &mut World, entity: Entity, entry: &Entry) {
    let parts = world.get::<RowParts>(entity).cloned().unwrap_or_default();
    match world.get::<RowKind>(entity).cloned() {
        Some(RowKind::Text { .. }) => w::set_text(world, entity, &entry.text),
        Some(RowKind::Built(_)) | None => {}
        Some(RowKind::Table { .. }) => {
            for (index, (text, value)) in parts
                .texts
                .iter()
                .zip(entry.columns.as_deref().unwrap_or_default())
                .enumerate()
            {
                match parts.subs.get(index).copied().flatten() {
                    Some(sub) => {
                        let (main, change) = split_change(value);
                        w::set_text(world, *text, main);
                        w::set_text(world, sub, change.unwrap_or(""));
                    }
                    None => w::set_text(world, *text, value),
                }
            }
            // The columns follow the window: a row built at one size keeps
            // its place in the table at another.
            let widths = widths_for(parts.cells.len(), compact(world));
            for (cell, width) in parts.cells.iter().zip(widths) {
                if let Some(mut node) = world.get_mut::<Node>(*cell)
                    && node.width != percent(width)
                {
                    node.width = percent(width);
                }
            }
            if let (Some(slot), Some(trend)) = (parts.mark, entry.trend) {
                set_mark(world, slot, trend_mark(trend));
            }
        }
        Some(RowKind::Action) | Some(RowKind::Marked { .. }) => {
            if let Some(action) = &entry.action
                && world.get::<LedgerAction>(entity).map(|held| &held.0) != Some(action)
            {
                world
                    .entity_mut(entity)
                    .insert(LedgerAction(action.clone()));
            }
            if world
                .get::<AccessibleLabel>(entity)
                .map(|label| label.0.as_str())
                != Some(entry.text.as_str())
            {
                world
                    .entity_mut(entity)
                    .insert(AccessibleLabel::new(&entry.text));
            }
            for (text, line) in parts.texts.iter().zip(entry.text.split('\n')) {
                w::set_text(world, *text, line);
            }
            let shape = entry
                .shape
                .or(entry.indicator.map(need_status))
                .map(|status| (Icon::status(status), status));
            if let (Some(slot), Some(shape)) = (parts.mark, shape) {
                set_mark(world, slot, shape);
            }
        }
    }
    // What a row says about itself on hover, where the page gave it words.
    match &entry.tip {
        Some(tip) if world.get::<Tooltip>(entity) != Some(tip) => {
            if world.get::<Hovered>(entity).is_none() {
                world.entity_mut(entity).insert(Hovered::default());
            }
            world.entity_mut(entity).insert(tip.clone());
        }
        _ => {}
    }
}

/// Reuse rows by key so focus, scroll position and hover survive a refresh.
/// Returns the rows now under `body`, by key.
pub(crate) fn reconcile_rows(
    world: &mut World,
    body: Entity,
    old: BTreeMap<String, Entity>,
    entries: Vec<Entry>,
) -> BTreeMap<String, Entity> {
    let mut live = BTreeMap::new();
    let mut order = Vec::new();
    for entry in entries {
        let wanted = kind_of(&entry);
        let entity = old
            .get(&entry.key)
            .copied()
            .filter(|entity| world.get::<RowKind>(*entity) == Some(&wanted))
            .unwrap_or_else(|| {
                let entity = build(world, body, &entry);
                // Table rows touch; everything else keeps a gap above it.
                let above = match &wanted {
                    RowKind::Table { heading: false, .. } => 0.0,
                    RowKind::Table { heading: true, .. } => 4.0,
                    RowKind::Marked { .. } => 2.0,
                    RowKind::Text { heading: true } => 12.0,
                    RowKind::Built(_) if entry.extra == Some(Extra::Group) => 0.0,
                    RowKind::Built(_) if matches!(entry.extra, Some(Extra::Meter { .. })) => 3.0,
                    _ => 6.0,
                };
                if let Some(mut node) = world.get_mut::<Node>(entity) {
                    node.margin = UiRect::top(px(above));
                }
                entity
            });
        update(world, entity, &entry);
        live.insert(entry.key, entity);
        order.push(entity);
    }
    for (key, entity) in old {
        if live.get(&key) != Some(&entity) {
            if world
                .get_resource::<InputFocus>()
                .is_some_and(|focus| focus.get() == Some(entity))
            {
                world.resource_mut::<InputFocus>().clear();
            }
            if world.get_entity(entity).is_ok() {
                world.despawn(entity);
            }
        }
    }
    if world
        .get::<Children>(body)
        .is_none_or(|children| children.as_ref() != order.as_slice())
    {
        world.entity_mut(body).replace_children(&order);
    }
    live
}

/// The ledger's own body.
pub(super) fn reconcile(world: &mut World, entries: Vec<Entry>) {
    let Some(body) = world
        .query_filtered::<Entity, With<LedgerBody>>()
        .iter(world)
        .next()
    else {
        return;
    };
    // A good's card sits under its own row (2.3); nothing is pinned to the
    // foot any more, and what was is taken away.
    let pinned: Vec<Entry> = Vec::new();
    let old = std::mem::take(&mut world.resource_mut::<Ledger>().rows);
    let live = reconcile_rows(world, body, old, entries);
    world.resource_mut::<Ledger>().rows = live;
    let Some(foot) = world
        .query_filtered::<Entity, With<LedgerFooter>>()
        .iter(world)
        .next()
    else {
        return;
    };
    let old = std::mem::take(&mut world.resource_mut::<Ledger>().pinned);
    let live = reconcile_rows(world, foot, old, pinned);
    world.resource_mut::<Ledger>().pinned = live;
}

/// The icon of a ledger tab.
#[derive(Component)]
pub(super) struct TabIcon;

/// A tab of the ledger's strip.
pub(super) fn tab(world: &mut World, strip: Entity, name: &str, picture: Icon, action: Action) {
    let entity = w::tab(
        world,
        strip,
        name,
        Some(picture),
        Reach::Tab(0),
        (LedgerControl, LedgerAction(action)),
    );
    // Five tabs share 400 px at the Standard width.
    world.get_mut::<Node>(entity).expect("tab").padding = UiRect::horizontal(px(4));
    // In the Compact column they keep their names and give up their icons.
    let pictures: Vec<Entity> = world
        .get::<Children>(entity)
        .map(|children| children.to_vec())
        .unwrap_or_default()
        .into_iter()
        .filter(|child| world.get::<Text>(*child).is_none())
        .collect();
    for picture in pictures {
        world.entity_mut(picture).insert(TabIcon);
    }
    debug_assert!(world.get::<TabIndex>(entity).is_some());
}
