//! The building card: the right column's panel on a site, a home, a
//! workplace, a store or a stall. It reads the simulation and records nothing.
use super::model::{Card, CardBody, CardTab, Stage};
use super::palette::BuildAction;
use super::{Action, BuildTool};
use crate::game_ui::icon::{self, Icon};
use crate::game_ui::scale;
use crate::game_ui::tokens::{self as t, Status, Surface};
use crate::game_ui::tooltip::Tooltip;
use crate::game_ui::widgets::{self as w, Bar, Reach, Tone};
use crate::island_shell::model::{Column, column_width};
use bevy::prelude::*;
use npc_framework::prelude::SimId;
use npc_framework::view_helpers::ViewSystems;

/// Seconds between two refreshes of an unchanged card.
const REFRESH_SECS: f32 = 0.25;

#[derive(Component)]
pub(crate) struct CardRoot;
#[derive(Component)]
struct TitleText;
#[derive(Component)]
struct KindText;
#[derive(Component)]
struct IconSlot;
/// Holds the status badge of a site: shape, colour and word.
#[derive(Component)]
struct StatusSlot;
#[derive(Component)]
struct ProgressRow;
#[derive(Component)]
struct Lines;
/// The card's tab strip; hidden for a card with one view.
#[derive(Component)]
struct TabStrip;
#[derive(Component)]
struct TabButton(CardTab);
#[derive(Component)]
struct TabLabel(CardTab);

#[derive(Resource, Default)]
struct Shown {
    subject: Option<SimId>,
    elapsed: f32,
    bar: Option<Bar>,
    /// What the rebuilt parts last showed.
    icon: Option<Icon>,
    status: Option<(Status, String)>,
    lines: Vec<String>,
    /// The subject the open tab belongs to: another building starts on Overview.
    tab_of: Option<SimId>,
}

pub(super) fn plugin(app: &mut App) {
    app.init_resource::<Shown>()
        .add_systems(Startup, spawn)
        .add_systems(
            Update,
            refresh.in_set(ViewSystems::Present).after(super::process),
        );
}

/// The building card for a test that walks the Tab ring.
#[cfg(test)]
pub(crate) fn spawn_for_tests(world: &mut World) {
    world.init_resource::<Shown>();
    spawn(world);
}

fn spawn(world: &mut World) {
    let root = world
        .spawn((
            Name::new("Building card"),
            CardRoot,
            crate::game_ui::sound::PanelSound::default(),
            crate::island_shell::focus::FocusRegion::Inspector,
            GlobalZIndex(t::layer::PANEL),
            w::panel(Node {
                position_type: PositionType::Absolute,
                top: px(t::TOP_BAR + t::SCREEN_MARGIN),
                right: px(t::SCREEN_MARGIN),
                width: px(t::COLUMN_INSPECTOR),
                max_width: percent(94),
                row_gap: px(8),
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
                align_items: AlignItems::Center,
                column_gap: px(8),
                min_height: px(32),
                flex_shrink: 0.0,
                ..default()
            },
        ))
        .id();
    world.spawn((
        ChildOf(header),
        IconSlot,
        Node {
            width: px(20),
            height: px(20),
            flex_shrink: 0.0,
            ..default()
        },
    ));
    let who = world
        .spawn((
            ChildOf(header),
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
    let title = w::label(world, who, "", t::HEADING, Tone::text(Surface::Paper));
    world.entity_mut(title).insert(TitleText);
    let kind = w::label(world, who, "", t::DENSE, Tone::secondary(Surface::Paper));
    world.entity_mut(kind).insert(KindText);
    let locate = w::icon_button(
        world,
        header,
        "Show on the island",
        Icon::Locate,
        Surface::Paper,
        Reach::Ring,
        BuildAction(Action::FocusCard),
    );
    world.entity_mut(locate).insert(Tooltip::new(
        "Show on the island",
        "Bring the camera to this place.",
    ));
    let close = w::icon_button(
        world,
        header,
        "Close card",
        Icon::Close,
        Surface::Paper,
        Reach::Ring,
        BuildAction(Action::CloseCard),
    );
    world
        .entity_mut(close)
        .insert(Tooltip::new("Close", "Close the card.").key("Esc"));
    world.spawn((
        ChildOf(root),
        StatusSlot,
        Node {
            flex_shrink: 0.0,
            ..default()
        },
    ));
    let progress = world
        .spawn((
            ChildOf(root),
            ProgressRow,
            Node {
                flex_direction: FlexDirection::Column,
                flex_shrink: 0.0,
                ..default()
            },
        ))
        .id();
    let bar = w::progress_bar(world, progress, Surface::Paper, 0.0, Some("0%"));
    world.resource_mut::<Shown>().bar = Some(bar);
    // Overview, then who lives or works there, or what is on the shelves.
    let strip = w::tabs(world, root);
    world.entity_mut(strip).insert(TabStrip);
    for tab in [CardTab::Overview, CardTab::People, CardTab::Stock] {
        let button = w::tab(
            world,
            strip,
            match tab {
                CardTab::Overview => "Overview",
                CardTab::People => "People",
                CardTab::Stock => "Stock",
            },
            None,
            Reach::Ring,
            (BuildAction(Action::CardTab(tab)), TabButton(tab)),
        );
        world.get_mut::<Node>(button).expect("tab").padding = UiRect::horizontal(px(4));
        let labels: Vec<Entity> = world
            .get::<Children>(button)
            .map(|children| children.to_vec())
            .unwrap_or_default()
            .into_iter()
            .filter(|child| world.get::<Text>(*child).is_some())
            .collect();
        for label in labels {
            world.entity_mut(label).insert(TabLabel(tab));
        }
    }
    world.spawn((
        ChildOf(root),
        Lines,
        Node {
            flex_direction: FlexDirection::Column,
            row_gap: px(4),
            ..default()
        },
    ));
}

fn set_text<M: Component>(world: &mut World, value: &str) {
    for mut text in world.query_filtered::<&mut Text, With<M>>().iter_mut(world) {
        if text.0 != value {
            text.0 = value.into();
        }
    }
}

fn show<M: Component>(world: &mut World, shown: bool) {
    let display = if shown { Display::Flex } else { Display::None };
    for mut node in world.query_filtered::<&mut Node, With<M>>().iter_mut(world) {
        if node.display != display {
            node.display = display;
        }
    }
}

/// The picture of the card's subject and, for a site, how it stands.
pub(crate) fn look(card: &Card) -> (Icon, Option<(Status, String)>) {
    match &card.body {
        CardBody::Site(site) => {
            let (status, word) = match site.stage() {
                _ if site.progress_permille() >= 1000 => (Status::Good, "Finished"),
                // A day without a delivery, or a fund that cannot pay.
                Stage::Materials if site.waiting_days > 0 || site.unfunded => {
                    (Status::Warn, "Waiting")
                }
                Stage::Materials => (Status::Info, "Collecting materials"),
                Stage::Ready => (Status::Info, "Ready to build"),
                Stage::Building => (Status::Info, "Under way"),
            };
            (Icon::BuildingSite, Some((status, word.to_owned())))
        }
        CardBody::Home { .. } => (Icon::Homes, None),
        CardBody::Workplace { trade, .. } => (Icon::of_trade(trade).unwrap_or(Icon::Work), None),
        CardBody::Depot { stall: true, .. } => (Icon::Market, None),
        CardBody::Depot { stall: false, .. } => (Icon::Storage, None),
    }
}

fn refresh(world: &mut World) {
    let delta = world.get_resource::<Time>().map_or(0.0, Time::delta_secs);
    let subject = super::card_subject(world);
    {
        let mut shown = world.resource_mut::<Shown>();
        shown.elapsed += delta;
        let changed = shown.subject != subject;
        shown.subject = subject;
        if !changed && shown.elapsed < REFRESH_SECS {
            return;
        }
        shown.elapsed = 0.0;
    }
    let card = subject.and_then(|id| super::card(world, id));
    // The right column: under the bar, above the docks, at the spec's width.
    let layout = scale::layout(world);
    let top = crate::island_shell::content_top(world);
    let bottom = crate::island_shell::content_bottom(world);
    let width = column_width(Column::Inspector, layout.class);
    for mut node in world
        .query_filtered::<&mut Node, With<CardRoot>>()
        .iter_mut(world)
    {
        let display = if card.is_some() {
            Display::Flex
        } else {
            Display::None
        };
        if node.display != display {
            node.display = display;
        }
        if node.top != px(top) {
            node.top = px(top);
        }
        if node.width != px(width) {
            node.width = px(width);
        }
        let most = px((layout.height - top - bottom).max(160.0));
        if node.max_height != most {
            node.max_height = most;
        }
    }
    let Some(card) = card else {
        // The subject is gone: nothing keeps the card open.
        if subject.is_some() {
            world.resource_mut::<BuildTool>().card = None;
        }
        return;
    };
    set_text::<TitleText>(world, &card.title);
    set_text::<KindText>(world, &card.kind_line());
    let (picture, status) = look(&card);
    if world.resource::<Shown>().icon != Some(picture) {
        let slots: Vec<Entity> = world
            .query_filtered::<Entity, With<IconSlot>>()
            .iter(world)
            .collect();
        for slot in slots {
            world.entity_mut(slot).despawn_children();
            icon::icon(world, slot, picture, 20.0, Tone::text(Surface::Paper));
        }
        world.resource_mut::<Shown>().icon = Some(picture);
    }
    if world.resource::<Shown>().status != status {
        let slots: Vec<Entity> = world
            .query_filtered::<Entity, With<StatusSlot>>()
            .iter(world)
            .collect();
        for slot in slots {
            world.entity_mut(slot).despawn_children();
            if let Some((status, word)) = &status {
                w::badge(world, slot, Surface::Paper, *status, word);
            }
        }
        world.resource_mut::<Shown>().status = status.clone();
    }
    show::<StatusSlot>(world, status.is_some());
    let permille = card.bar_permille();
    show::<ProgressRow>(world, permille.is_some());
    if let (Some(permille), Some(bar)) = (permille, world.resource::<Shown>().bar) {
        w::set_bar(
            world,
            bar,
            permille as f32 / 1000.0,
            Some(&format!("{}%", permille / 10)),
        );
    }
    // Another building starts on its Overview; a tab it lacks shows that too.
    if world.resource::<Shown>().tab_of != subject {
        world.resource_mut::<Shown>().tab_of = subject;
        world.resource_mut::<BuildTool>().card_tab = CardTab::Overview;
    }
    let tabs = card.tabs();
    let open = Some(world.resource::<BuildTool>().card_tab)
        .filter(|tab| tabs.contains(tab))
        .unwrap_or_default();
    show::<TabStrip>(world, tabs.len() > 1);
    let buttons: Vec<(Entity, CardTab)> = world
        .query::<(Entity, &TabButton)>()
        .iter(world)
        .map(|(entity, tab)| (entity, tab.0))
        .collect();
    for (entity, tab) in buttons {
        w::show(world, entity, tabs.contains(&tab));
        w::set_state(world, entity, tab == open, false);
    }
    let labels: Vec<(Entity, CardTab)> = world
        .query::<(Entity, &TabLabel)>()
        .iter(world)
        .map(|(entity, tab)| (entity, tab.0))
        .collect();
    for (entity, tab) in labels {
        w::set_text(world, entity, card.tab_label(tab));
    }
    let lines = card.tab_lines(open);
    if world.resource::<Shown>().lines != lines {
        let holders: Vec<Entity> = world
            .query_filtered::<Entity, With<Lines>>()
            .iter(world)
            .collect();
        for holder in holders {
            world.entity_mut(holder).despawn_children();
            for line in &lines {
                // "Beds 1/2": a fact per line; a "Label: value" line keeps its label.
                w::paragraph(world, holder, line, t::DENSE, Tone::text(Surface::Paper));
            }
        }
        world.resource_mut::<Shown>().lines = lines;
    }
}
