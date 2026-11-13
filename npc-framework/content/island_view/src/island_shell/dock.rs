//! The bottom docks of Oversee: Overlays and Places at the left, the build
//! categories in the middle and the ledger's pages at the right.
//!
//! A dock tile opens something in a fixed place and closes it again: the
//! build menu, a ledger page in the single right column, or a small list
//! above its own button. View state only; the one effect on the world is a
//! camera move.
use super::focus::FocusRegion;
use super::model::{self, LedgerPage};
use super::{Popover, ShellState};
use crate::game_ui::icon::{self, Icon};
use crate::game_ui::scale::{self, LayoutClass};
use crate::game_ui::tokens::{self as t, Status, Surface};
use crate::game_ui::tooltip::Tooltip;
use crate::game_ui::widgets::{self as w, DockTile, Part, Reach, Tone};
use crate::island_build::Group;
use bevy::input_focus::InputFocus;
use bevy::prelude::*;
use bevy::ui::InteractionDisabled;
use bevy::ui_widgets::Activate;
use island_content::village;
use npc_framework::prelude::{SimId, SimIndex, SimPos};
use npc_framework::view_helpers::sim_to_world;

/// The overlays of the Overlays list. Only Trades exists today; the others
/// are listed, disabled, so the control needs no redesign when they land.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Overlay {
    Trades,
    HomesAndBeds,
    OpenJobs,
    ResourcePatches,
    BuildingGrid,
}

impl Overlay {
    pub(crate) const ALL: [Self; 5] = [
        Self::Trades,
        Self::HomesAndBeds,
        Self::OpenJobs,
        Self::ResourcePatches,
        Self::BuildingGrid,
    ];
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Trades => "Trades",
            Self::HomesAndBeds => "Homes and beds",
            Self::OpenJobs => "Open jobs",
            Self::ResourcePatches => "Resource patches",
            Self::BuildingGrid => "Building grid",
        }
    }
    pub(crate) fn icon(self) -> Icon {
        match self {
            Self::Trades => Icon::Jobs,
            Self::HomesAndBeds => Icon::Rest,
            Self::OpenJobs => Icon::Jobs,
            Self::ResourcePatches => Icon::ResourcePatches,
            Self::BuildingGrid => Icon::BuildingSite,
        }
    }
    /// Whether the game can draw this overlay yet.
    pub(crate) fn available(self) -> bool {
        self == Self::Trades
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DockAction {
    Overlays,
    Places,
    /// A build category.
    Build(Group),
    Ledger(LedgerPage),
    /// The one ledger tile of the Compact layout: the last page, or close.
    LedgerLast,
    Overlay(Overlay),
    /// Frame a village; `None` is the whole island.
    Place(Option<SimId>),
}

#[derive(Component, Clone, Copy)]
pub(crate) struct DockControl(DockAction);
/// Any of the three docks.
#[derive(Component)]
pub(crate) struct DockRoot;
/// The list a left-dock button opened.
#[derive(Component)]
pub(crate) struct PopoverRoot;
#[derive(Component, Clone, Copy, PartialEq, Eq)]
enum Dock {
    Left,
    Build,
    Ledger,
}
#[derive(Component)]
struct BuildTiles;
/// A page tile of the ledger dock, and the one tile that stands for all five
/// where there is no room for them.
#[derive(Component)]
struct PageTile;
#[derive(Component)]
struct LedgerTile;
#[derive(Component)]
struct OverlayList;
#[derive(Component)]
struct PlaceList;
/// The tick beside the active overlay.
#[derive(Component)]
struct OverlayTick(Overlay);

#[derive(Resource, Default)]
pub(crate) struct Docks {
    pub(crate) requests: Vec<DockAction>,
    tiles: Vec<(DockAction, Entity)>,
    projects: Option<(Entity, Entity)>,
    /// Build categories on the dock, and the world they were read from.
    groups: Option<(u64, Vec<Group>)>,
    places: Option<u64>,
    class: Option<LayoutClass>,
}

/// The dock label of a build category.
pub(crate) fn group_label(group: Group) -> &'static str {
    group.dock_label()
}

fn group_icon(group: Group) -> Icon {
    group.icon()
}

fn dock(world: &mut World, name: &str, kind: Dock, region: FocusRegion) -> Entity {
    world
        .spawn((
            Name::new(name.to_owned()),
            DockRoot,
            kind,
            region,
            GlobalZIndex(t::layer::CHROME),
            w::chrome(
                Node {
                    position_type: PositionType::Absolute,
                    bottom: px(t::SCREEN_MARGIN),
                    height: px(t::DOCK),
                    padding: UiRect::all(px(5)),
                    column_gap: px(4),
                    align_items: AlignItems::Center,
                    display: Display::None,
                    ..default()
                },
                t::RADIUS_LG,
            ),
        ))
        .id()
}

fn tile(
    world: &mut World,
    parent: Entity,
    name: &str,
    picture: Icon,
    key: Option<&str>,
    action: DockAction,
    sentence: &str,
) -> DockTile {
    let tile = w::dock_tile(world, parent, name, picture, key, DockControl(action));
    let mut tooltip = Tooltip::new(name, sentence).icon(picture);
    if let Some(key) = key {
        tooltip = tooltip.key(key);
    }
    world.entity_mut(tile.entity).insert(tooltip);
    world
        .resource_mut::<Docks>()
        .tiles
        .push((action, tile.entity));
    tile
}

fn popover(world: &mut World, name: &str, region_marker: impl Bundle) -> Entity {
    world
        .spawn((
            Name::new(name.to_owned()),
            PopoverRoot,
            FocusRegion::Popover,
            region_marker,
            GlobalZIndex(t::layer::POPOVER),
            w::panel(Node {
                position_type: PositionType::Absolute,
                left: px(t::SCREEN_MARGIN),
                bottom: px(t::SCREEN_MARGIN + t::DOCK + 6.0),
                width: px(220),
                padding: UiRect::all(px(8)),
                row_gap: px(2),
                flex_direction: FlexDirection::Column,
                display: Display::None,
                ..default()
            }),
        ))
        .id()
}

fn popover_row(
    world: &mut World,
    parent: Entity,
    name: &str,
    picture: Icon,
    action: DockAction,
) -> Entity {
    let row = w::list_row(world, parent, name, false, Reach::Ring, DockControl(action));
    let picture = icon::icon(world, row, picture, 16.0, Tone::secondary(Surface::Paper));
    world.entity_mut(picture).insert(Part::Label);
    let text = w::label(world, row, name, t::DENSE, Tone::text(Surface::Paper));
    world.entity_mut(text).insert((
        Part::Label,
        Node {
            flex_grow: 1.0,
            ..default()
        },
    ));
    row
}

pub(crate) fn spawn(world: &mut World) {
    let left = dock(
        world,
        "Island dock: view",
        Dock::Left,
        FocusRegion::DockLeft,
    );
    world.get_mut::<Node>(left).expect("dock").left = px(t::SCREEN_MARGIN);
    tile(
        world,
        left,
        "Overlays",
        Icon::Overlays,
        Some("O"),
        DockAction::Overlays,
        "Choose what the island view marks.",
    );
    tile(
        world,
        left,
        "Places",
        Icon::Places,
        Some("M"),
        DockAction::Places,
        "Bring the camera to a village.",
    );

    let build = dock(
        world,
        "Island dock: build",
        Dock::Build,
        FocusRegion::DockBuild,
    );
    let caption = world
        .spawn((
            ChildOf(build),
            Node {
                width: px(44),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: px(3),
                flex_shrink: 0.0,
                ..default()
            },
        ))
        .id();
    w::label(
        world,
        caption,
        "Build",
        t::CAPTION,
        Tone::secondary(Surface::Chrome),
    );
    w::keycap(world, caption, Surface::Chrome, "B");
    w::divider(world, build, Surface::Chrome, true);
    world.spawn((
        ChildOf(build),
        BuildTiles,
        Node {
            column_gap: px(4),
            align_items: AlignItems::Center,
            ..default()
        },
    ));

    let ledger = dock(
        world,
        "Island dock: ledger",
        Dock::Ledger,
        FocusRegion::DockLedger,
    );
    world.get_mut::<Node>(ledger).expect("dock").right = px(t::SCREEN_MARGIN);
    for page in LedgerPage::ALL {
        let tile = tile(
            world,
            ledger,
            page.label(),
            page.icon(),
            Some(page.key_cap()),
            DockAction::Ledger(page),
            page.summary(),
        );
        world.entity_mut(tile.entity).insert(PageTile);
        if page == LedgerPage::Projects {
            world.resource_mut::<Docks>().projects = Some((tile.badge, tile.badge_text));
        }
    }
    let all = tile(
        world,
        ledger,
        "Ledger",
        Icon::Ledger,
        Some("F11"),
        DockAction::LedgerLast,
        "Open the ledger on the page it showed last. Its tabs lead to the others.",
    );
    world.entity_mut(all.entity).insert(LedgerTile);
    w::show(world, all.entity, false);

    let overlays = popover(world, "Island overlays", OverlayList);
    w::label(
        world,
        overlays,
        "Overlays",
        t::CAPTION,
        Tone::caption(Surface::Paper),
    );
    for overlay in Overlay::ALL {
        let row = popover_row(
            world,
            overlays,
            overlay.label(),
            overlay.icon(),
            DockAction::Overlay(overlay),
        );
        if overlay.available() {
            let tick = world
                .spawn((
                    ChildOf(row),
                    OverlayTick(overlay),
                    Node {
                        width: px(14),
                        height: px(14),
                        ..default()
                    },
                ))
                .id();
            icon::icon(
                world,
                tick,
                Icon::Done,
                14.0,
                Tone::status(Surface::Paper, Status::Good),
            );
            world.entity_mut(row).insert(Tooltip::new(
                overlay.label(),
                "Colour each resident's marker by trade and show the key.",
            ));
        } else {
            icon::icon(
                world,
                row,
                Icon::Locked,
                12.0,
                Tone::caption(Surface::Paper),
            );
            world.entity_mut(row).insert((
                InteractionDisabled,
                Tooltip::new(overlay.label(), "This overlay is not in the game yet."),
            ));
        }
    }
    let places = popover(world, "Island places", PlaceList);
    w::label(
        world,
        places,
        "Places",
        t::CAPTION,
        Tone::caption(Surface::Paper),
    );
}

pub(crate) fn activate(
    event: On<Activate>,
    controls: Query<(&DockControl, Has<InteractionDisabled>)>,
    mut docks: ResMut<Docks>,
) {
    if let Ok((control, false)) = controls.get(event.entity) {
        docks.requests.push(control.0);
    }
}

/// F1 to F5 open the ledger's pages, O the overlays, M the places and F
/// follows the selected resident. Oversee only; handled keys are consumed.
pub(crate) fn keys(world: &mut World) {
    if !super::overseeing(world) {
        return;
    }
    let focused = world
        .get_resource::<InputFocus>()
        .and_then(|focus| focus.get());
    if focused.is_some_and(|entity| world.get::<bevy::text::EditableText>(entity).is_some()) {
        return;
    }
    let Some(keys) = world.get_resource::<ButtonInput<KeyCode>>() else {
        return;
    };
    if keys.any_pressed([
        KeyCode::ControlLeft,
        KeyCode::ControlRight,
        KeyCode::AltLeft,
        KeyCode::AltRight,
        KeyCode::SuperLeft,
        KeyCode::SuperRight,
    ]) {
        return;
    }
    // With `--dev-keys` the function keys belong to the developer panels.
    let function_keys = !world
        .get_resource::<crate::DeveloperKeys>()
        .is_some_and(|developer| developer.0);
    let mut pressed = Vec::new();
    if function_keys {
        for page in LedgerPage::ALL {
            if keys.just_pressed(page.key()) {
                pressed.push((page.key(), Some(DockAction::Ledger(page))));
            }
        }
    }
    if keys.just_pressed(KeyCode::KeyO) {
        pressed.push((KeyCode::KeyO, Some(DockAction::Overlays)));
    }
    if keys.just_pressed(KeyCode::KeyM) {
        pressed.push((KeyCode::KeyM, Some(DockAction::Places)));
    }
    let follow = keys.just_pressed(KeyCode::KeyF) && crate::resident_inspector::open(world);
    if follow {
        pressed.push((KeyCode::KeyF, None));
    }
    if pressed.is_empty() {
        return;
    }
    let mut keys = world.resource_mut::<ButtonInput<KeyCode>>();
    for (key, _) in &pressed {
        keys.reset(*key);
    }
    world
        .resource_mut::<Docks>()
        .requests
        .extend(pressed.into_iter().filter_map(|(_, action)| action));
    if follow {
        crate::resident_inspector::toggle_follow(world);
    }
}

fn settlements(world: &World) -> Vec<(SimId, String, SimPos)> {
    village::frontier::settlements::all(world)
        .into_iter()
        .filter_map(|(id, entity)| {
            Some((
                id,
                world
                    .get::<village::frontier::Settlement>(entity)?
                    .label
                    .clone(),
                *world.get::<SimPos>(entity)?,
            ))
        })
        .collect()
}

pub(crate) fn process(world: &mut World) {
    let requests = std::mem::take(&mut world.resource_mut::<Docks>().requests);
    if !super::overseeing(world) {
        return;
    }
    for request in requests {
        match request {
            DockAction::Overlays | DockAction::Places => {
                let wanted = if request == DockAction::Overlays {
                    Popover::Overlays
                } else {
                    Popover::Places
                };
                let mut state = world.resource_mut::<ShellState>();
                state.popover = if state.popover == wanted {
                    Popover::None
                } else {
                    wanted
                };
            }
            DockAction::Build(group) => crate::island_build::request_group(world, group),
            DockAction::Ledger(page) => crate::colony_panel::toggle_page(world, page),
            DockAction::LedgerLast => crate::colony_panel::toggle_last(world),
            DockAction::Overlay(overlay) => {
                if overlay == Overlay::Trades {
                    let shown = crate::island_atmosphere::trades_key(world);
                    crate::island_atmosphere::set_trades_key(world, !shown);
                }
            }
            DockAction::Place(place) => {
                match place {
                    None => crate::island_camera::overview(world, false),
                    Some(id) => {
                        let position = world
                            .get_resource::<SimIndex>()
                            .and_then(|index| index.get(id))
                            .and_then(|entity| world.get::<SimPos>(entity))
                            .copied();
                        if let Some(position) = position {
                            crate::island_camera::focus(world, sim_to_world(position), 65.0);
                        }
                    }
                }
                world.resource_mut::<ShellState>().popover = Popover::None;
            }
        }
    }
}

fn show_all<M: Component>(world: &mut World, shown: bool) {
    let display = if shown { Display::Flex } else { Display::None };
    for mut node in world.query_filtered::<&mut Node, With<M>>().iter_mut(world) {
        if node.display != display {
            node.display = display;
        }
    }
}

/// The build categories the world offers, as tiles. An empty category is
/// hidden, not disabled; a world that cannot build has no build dock.
fn refresh_build(world: &mut World) -> usize {
    if !crate::island_build::available(world) {
        return 0;
    }
    let epoch = npc_framework::agent_io::api::epoch(world);
    let groups = crate::island_build::groups(world);
    if world.resource::<Docks>().groups.as_ref() != Some(&(epoch, groups.clone())) {
        let Some(holder) = world
            .query_filtered::<Entity, With<BuildTiles>>()
            .iter(world)
            .next()
        else {
            return groups.len();
        };
        world.entity_mut(holder).despawn_children();
        world
            .resource_mut::<Docks>()
            .tiles
            .retain(|(action, _)| !matches!(action, DockAction::Build(_)));
        for group in &groups {
            tile(
                world,
                holder,
                group_label(*group),
                group_icon(*group),
                None,
                DockAction::Build(*group),
                "Open the build menu on this category.",
            );
        }
        let mut docks = world.resource_mut::<Docks>();
        docks.groups = Some((epoch, groups.clone()));
        docks.class = None;
    }
    groups.len()
}

fn refresh_places(world: &mut World) {
    let epoch = npc_framework::agent_io::api::epoch(world);
    if world.resource::<Docks>().places == Some(epoch) {
        return;
    }
    let Some(list) = world
        .query_filtered::<Entity, With<PlaceList>>()
        .iter(world)
        .next()
    else {
        return;
    };
    let old: Vec<Entity> = world
        .get::<Children>(list)
        .map(|children| children.iter().skip(1).collect())
        .unwrap_or_default();
    for entity in old {
        world.despawn(entity);
    }
    popover_row(
        world,
        list,
        "Whole island",
        Icon::WholeIsland,
        DockAction::Place(None),
    );
    for (id, name, _) in settlements(world) {
        popover_row(
            world,
            list,
            &name,
            Icon::Places,
            DockAction::Place(Some(id)),
        );
    }
    world.resource_mut::<Docks>().places = Some(epoch);
}

pub(crate) fn refresh(world: &mut World) {
    let gated = world
        .get_resource::<crate::FrontendGate>()
        .is_some_and(|gate| gate.0);
    let shown = super::overseeing(world) && !gated;
    if !shown {
        show_all::<DockRoot>(world, false);
        show_all::<PopoverRoot>(world, false);
        // A popover belongs to Oversee; the calendar stays with the bar.
        let mut state = world.resource_mut::<ShellState>();
        if matches!(state.popover, Popover::Overlays | Popover::Places) {
            state.popover = Popover::None;
        }
        return;
    }
    let build_tiles = refresh_build(world);
    let layout = scale::layout(world);
    let rects = model::dock_layout(layout.width, layout.class, build_tiles);
    let docks: Vec<(Entity, Dock)> = world
        .query::<(Entity, &Dock)>()
        .iter(world)
        .map(|(entity, dock)| (entity, *dock))
        .collect();
    for (entity, dock) in docks {
        let display = if dock == Dock::Build && build_tiles == 0 {
            Display::None
        } else {
            Display::Flex
        };
        let mut node = world.get_mut::<Node>(entity).expect("dock");
        if node.display != display {
            node.display = display;
        }
        if dock == Dock::Build && node.left != px(rects.build.0) {
            node.left = px(rects.build.0);
        }
    }
    if world.resource::<Docks>().class != Some(layout.class) {
        let tiles: Vec<Entity> = world
            .resource::<Docks>()
            .tiles
            .iter()
            .map(|(_, entity)| *entity)
            .collect();
        for entity in tiles {
            if let Some(mut node) = world.get_mut::<Node>(entity) {
                node.width = px(rects.tile);
            }
        }
        world.resource_mut::<Docks>().class = Some(layout.class);
    }
    // Compact: one Ledger tile; the pages are its tabs and F1 to F5.
    let one = model::ledger_tiles(layout.class) == 1;
    show_all::<PageTile>(world, !one);
    show_all::<LedgerTile>(world, one);
    let popover = world.resource::<ShellState>().popover;
    let page = crate::colony_panel::page(world);
    let building = crate::island_build::open_group(world);
    let tiles = world.resource::<Docks>().tiles.clone();
    for (action, entity) in tiles {
        let on = match action {
            DockAction::Overlays => popover == Popover::Overlays,
            DockAction::Places => popover == Popover::Places,
            DockAction::Build(group) => building == Some(group),
            DockAction::Ledger(wanted) => page == Some(wanted),
            DockAction::LedgerLast => page.is_some(),
            DockAction::Overlay(_) | DockAction::Place(_) => false,
        };
        w::set_state(world, entity, on, false);
    }
    // The count of open sites on the Projects tile.
    let sites = if crate::island_build::available(world) {
        village::civic::sites(world).len()
    } else {
        0
    };
    if let Some((badge, text)) = world.resource::<Docks>().projects {
        w::set_text(world, text, &sites.to_string());
        w::show(world, badge, sites > 0);
    }
    show_all::<OverlayList>(world, popover == Popover::Overlays);
    show_all::<PlaceList>(world, popover == Popover::Places);
    if popover == Popover::Places {
        refresh_places(world);
    }
    if popover == Popover::Overlays {
        let trades = crate::island_atmosphere::trades_key(world);
        let ticks: Vec<(Entity, Overlay)> = world
            .query::<(Entity, &OverlayTick)>()
            .iter(world)
            .map(|(entity, tick)| (entity, tick.0))
            .collect();
        for (entity, overlay) in ticks {
            w::show(world, entity, overlay == Overlay::Trades && trades);
        }
        let rows: Vec<(Entity, DockAction)> = world
            .query::<(Entity, &DockControl)>()
            .iter(world)
            .map(|(entity, control)| (entity, control.0))
            .collect();
        for (entity, action) in rows {
            if action == DockAction::Overlay(Overlay::Trades) {
                w::set_state(world, entity, trades, false);
            }
        }
    }
    // The Places list opens above its own button.
    let offset = t::SCREEN_MARGIN + 5.0 + rects.tile + 4.0;
    for mut node in world
        .query_filtered::<&mut Node, With<PlaceList>>()
        .iter_mut(world)
    {
        if node.left != px(offset) {
            node.left = px(offset);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::island_shell::model::{Column, ColumnEvent, ColumnState};

    fn fixture(scenario: &str) -> App {
        let mut app = island_content::new_sim_app(7, scenario);
        app.init_resource::<crate::IslandMode>()
            .init_resource::<InputFocus>()
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<ShellState>()
            .init_resource::<Docks>()
            .insert_resource(crate::PlayerExperience { enabled: true });
        spawn(app.world_mut());
        app
    }

    fn shown(app: &mut App, dock: Dock) -> bool {
        let world = app.world_mut();
        world
            .query::<(&Dock, &Node)>()
            .iter(world)
            .any(|(found, node)| *found == dock && node.display != Display::None)
    }

    fn press(app: &mut App, key: KeyCode) {
        let mut input = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        input.reset_all();
        input.press(key);
        keys(app.world_mut());
    }

    #[test]
    fn the_docks_belong_to_oversee_and_hide_under_a_menu() {
        let mut app = fixture("living_world");
        refresh(app.world_mut());
        assert!(shown(&mut app, Dock::Left) && shown(&mut app, Dock::Ledger));
        assert!(
            !shown(&mut app, Dock::Build),
            "a world that cannot build has no build dock"
        );
        // Five ledger tiles with their keys, two view tiles.
        let names: Vec<String> = {
            let world = app.world_mut();
            let docks = world.resource::<Docks>().tiles.clone();
            docks
                .iter()
                .map(|(_, entity)| world.get::<Name>(*entity).unwrap().to_string())
                .collect()
        };
        assert_eq!(
            names,
            [
                "Game: Overlays",
                "Game: Places",
                "Game: Village",
                "Game: Economy",
                "Game: People",
                "Game: Projects",
                "Game: Journal",
                "Game: Ledger"
            ]
        );
        // The one Ledger tile stands in for the five only in the Compact layout.
        let displayed = |app: &mut App, name: &str| {
            let world = app.world_mut();
            let tiles = world.resource::<Docks>().tiles.clone();
            tiles.iter().any(|(_, entity)| {
                world.get::<Name>(*entity).unwrap().as_str() == name
                    && world.get::<Node>(*entity).unwrap().display != Display::None
            })
        };
        assert!(displayed(&mut app, "Game: Village") && !displayed(&mut app, "Game: Ledger"));
        for mode in [crate::IslandMode::Editor, crate::IslandMode::Character] {
            app.insert_resource(mode);
            refresh(app.world_mut());
            assert!(!shown(&mut app, Dock::Left) && !shown(&mut app, Dock::Ledger));
        }
        app.insert_resource(crate::IslandMode::Inspector);
        app.insert_resource(crate::FrontendGate(true));
        refresh(app.world_mut());
        assert!(
            !shown(&mut app, Dock::Ledger),
            "hidden under the pause menu"
        );
    }

    #[test]
    fn keys_open_popovers_and_are_consumed_only_while_overseeing() {
        let mut app = fixture("living_world");
        press(&mut app, KeyCode::KeyO);
        assert!(
            !app.world()
                .resource::<ButtonInput<KeyCode>>()
                .just_pressed(KeyCode::KeyO)
        );
        process(app.world_mut());
        assert_eq!(
            app.world().resource::<ShellState>().popover,
            Popover::Overlays
        );
        // One popover at a time; the same key closes it again.
        press(&mut app, KeyCode::KeyM);
        process(app.world_mut());
        assert_eq!(
            app.world().resource::<ShellState>().popover,
            Popover::Places
        );
        press(&mut app, KeyCode::KeyM);
        process(app.world_mut());
        assert_eq!(app.world().resource::<ShellState>().popover, Popover::None);
        // A function key asks for its page.
        press(&mut app, KeyCode::F3);
        assert_eq!(
            app.world().resource::<Docks>().requests,
            [DockAction::Ledger(LedgerPage::People)]
        );
        app.world_mut().resource_mut::<Docks>().requests.clear();
        // With developer keys the function keys are left alone.
        app.insert_resource(crate::DeveloperKeys(true));
        press(&mut app, KeyCode::F3);
        assert!(app.world().resource::<Docks>().requests.is_empty());
        assert!(
            app.world()
                .resource::<ButtonInput<KeyCode>>()
                .just_pressed(KeyCode::F3)
        );
        app.insert_resource(crate::DeveloperKeys(false));
        // Shape keeps M, O and the function keys for itself.
        app.insert_resource(crate::IslandMode::Editor);
        press(&mut app, KeyCode::KeyO);
        assert!(app.world().resource::<Docks>().requests.is_empty());
        assert!(
            app.world()
                .resource::<ButtonInput<KeyCode>>()
                .just_pressed(KeyCode::KeyO)
        );
    }

    #[test]
    fn overlays_list_what_exists_and_say_what_does_not() {
        let mut app = fixture("living_world");
        let world = app.world_mut();
        let rows: Vec<(Overlay, bool)> = world
            .query::<(&DockControl, Has<InteractionDisabled>)>()
            .iter(world)
            .filter_map(|(control, disabled)| match control.0 {
                DockAction::Overlay(overlay) => Some((overlay, disabled)),
                _ => None,
            })
            .collect();
        assert_eq!(rows.len(), Overlay::ALL.len());
        for (overlay, disabled) in rows {
            assert_eq!(disabled, !overlay.available(), "{overlay:?}");
        }
        // A disabled row says why in its tooltip.
        let reasons = world
            .query::<(&DockControl, &Tooltip)>()
            .iter(world)
            .filter(|(control, tooltip)| {
                matches!(control.0, DockAction::Overlay(overlay) if !overlay.available())
                    && tooltip.body.contains("not in the game yet")
            })
            .count();
        assert_eq!(reasons, 4);
    }

    #[test]
    fn the_column_model_drives_one_right_column() {
        // The dock asks; the column model answers: see `model` for the rules.
        let state = ColumnState::default().step(ColumnEvent::Page(LedgerPage::Journal));
        assert_eq!(state.column(), Column::Ledger(LedgerPage::Journal));
        assert_eq!(group_label(Group::Workplaces), "Work");
    }
}
