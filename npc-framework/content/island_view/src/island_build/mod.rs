//! Building in Oversee: the build palette and its placement ghost, the
//! drawing of construction sites, stores, stalls and finished buildings, the
//! projects data behind the ledger and the building card.
//!
//! Everything here reads the simulation through `village::civic` and changes
//! it only by recording `PlaceBuilding` (the ledger records `CancelBuilding`).
//! The ghost asks the simulation's own `placement_check`, only when the cell,
//! the turn or the building under the cursor changes (and twice a second, so
//! a resident walking through shows up). Tool state, the decision log and
//! the delivery watch are view state scoped to the session epoch: never
//! saved, hashed or read by the simulation.
pub(crate) mod card;
pub(crate) mod model;
pub(crate) mod palette;
mod scene;
#[cfg(test)]
mod tests;

use crate::{IslandMode, game_ui as ui};
use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use island_content::village::{
    self, VillagePlace, Villager, catalog, civic,
    colony::Household,
    frontier::{self, settlements::WORKPLACE_OFFSET_MM},
    production::JobIntent,
};
use model::{Card, CardBody, Decision, Ghost, PaletteItem, Shelf, SiteRow, Tint};
use npc_framework::kernel::commands::{CommandQueue, SkippedCommands, submit_deferred};
use npc_framework::kernel::journal::{Journal, JournalKind};
use npc_framework::prelude::{
    SimClock, SimCommand, SimConfig, SimControl, SimId, SimIndex, SimPos,
};
use npc_framework::view_helpers::{ViewSystems, command_ui::DragBox, sim_to_world};
use serde_json::Value;
use std::collections::BTreeMap;
// The tests aim at a founding home behind its door.
#[cfg(test)]
use island_content::village::frontier::settlements::HOME_OFFSET_MM;

pub(crate) use card::CardRoot;
pub(crate) use model::Group;

/// Seconds between two placement checks of an unchanged ghost.
const RECHECK_SECS: f32 = 0.5;
/// Seconds a refusal stays beside the cursor.
const REJECTION_SECS: f32 = 6.0;
/// Seconds between two reads of the open sites for the delivery watch.
const WATCH_SECS: f32 = 0.5;
/// Planner decisions kept for the projects panel.
const LOG_CAP: usize = 40;
/// Camera distance when a site or a building is focused.
pub(crate) const FOCUS_DISTANCE: f32 = 26.0;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Action {
    /// Open or close the palette (the bar's Build button, or B).
    Toggle,
    Close,
    Group(Group),
    /// Choose a building; choosing the chosen one again puts it down.
    Pick(String),
    Rotate,
    CancelTool,
    Place,
    CloseCard,
    FocusCard,
    /// A tab of the building card.
    CardTab(model::CardTab),
}

#[derive(Clone, Debug, PartialEq)]
struct GhostKey {
    building: String,
    cell: (i32, i32),
    rotation: u8,
    revision: u64,
}

/// The build tool, the palette and the building card: presentation only.
#[derive(Resource)]
pub(crate) struct BuildTool {
    pub(crate) open: bool,
    pub(crate) group: Group,
    /// Key of the building being placed.
    pub(crate) selected: Option<String>,
    pub(crate) rotation: u8,
    pub(crate) hover: Option<(i32, i32)>,
    pub(crate) ghost: Option<Ghost>,
    ghost_key: Option<GhostKey>,
    since_check: f32,
    pub(crate) requests: Vec<Action>,
    /// Recorded placements the simulation has not answered yet.
    inflight: Vec<((u64, u32), String)>,
    rejection: Option<(String, f32)>,
    pub(crate) status: String,
    epoch: Option<u64>,
    /// The palette's buildings, read once per world.
    items: Vec<PaletteItem>,
    items_epoch: Option<u64>,
    /// The site or building the card shows.
    pub(crate) card: Option<SimId>,
    /// The card's open tab; a new subject starts on Overview.
    pub(crate) card_tab: model::CardTab,
    /// A click on the world this frame, to be resolved after selection ran.
    click: Option<Ray3d>,
    /// This frame's outline: `(centre, size, colour)` boxes and lines.
    cubes: Vec<(Vec3, Vec3, Color)>,
    lines: Vec<(Vec3, Vec3, Color)>,
}

impl Default for BuildTool {
    fn default() -> Self {
        Self {
            open: false,
            group: Group::Homes,
            selected: None,
            rotation: 0,
            hover: None,
            ghost: None,
            ghost_key: None,
            since_check: 0.0,
            requests: Vec::new(),
            inflight: Vec::new(),
            rejection: None,
            status: String::new(),
            epoch: None,
            items: Vec::new(),
            items_epoch: None,
            card: None,
            card_tab: model::CardTab::Overview,
            click: None,
            cubes: Vec::new(),
            lines: Vec::new(),
        }
    }
}

impl BuildTool {
    /// The refusal to show beside the cursor: the ghost's own first, then the
    /// latest refusal the simulation reported for a recorded placement.
    pub(crate) fn cursor_reason(&self) -> Option<&str> {
        self.ghost
            .as_ref()
            .and_then(|ghost| ghost.blocked.as_deref())
            .or(self.rejection.as_ref().map(|(reason, _)| reason.as_str()))
    }
    pub(crate) fn queued(&self) -> usize {
        self.inflight.len()
    }
}

/// Planner decisions of this session and when each open site last received
/// a delivery. Derived from the journal and from read-only site views.
#[derive(Resource, Default)]
pub(crate) struct CivicLog {
    epoch: Option<u64>,
    cursor: Option<(u64, u32)>,
    pub(crate) decisions: Vec<Decision>,
    /// Site → (units delivered, tick of the last change).
    watch: BTreeMap<SimId, (u32, u64)>,
    elapsed: f32,
    /// Bumped when the log or the watch changes what a panel would show.
    pub(crate) revision: u64,
}

pub(crate) fn plugin(app: &mut App) {
    if !app.world().contains_resource::<civic::CivicState>() {
        return;
    }
    ui::install(app);
    app.init_resource::<BuildTool>()
        .init_resource::<CivicLog>()
        .init_resource::<IslandMode>()
        .add_systems(
            Update,
            keys.in_set(ViewSystems::OverlayInput)
                .before(crate::island_shell::ShellInput)
                .before(crate::resident_inspector::keys),
        )
        .add_systems(
            Update,
            pointer
                .in_set(ViewSystems::OverlayInput)
                .after(crate::colony_panel::pointer)
                .after(crate::shape_tools::pointer)
                .run_if(not(crate::player_hud::active)),
        )
        .add_systems(
            Update,
            (track, process, gizmo)
                .chain()
                .in_set(ViewSystems::Present)
                .after(crate::colony_panel::refresh)
                .before(crate::island_camera::CameraFraming),
        );
    palette::plugin(app);
    card::plugin(app);
    scene::plugin(app);
}

/// Whether the build tools exist in this world at all.
pub(crate) fn available(world: &World) -> bool {
    world.contains_resource::<BuildTool>()
}

fn overseeing(world: &World) -> bool {
    world.get_resource::<IslandMode>() == Some(&IslandMode::Inspector)
        && !crate::player_hud::active(world)
}

/// Whether the palette is on screen, for the bar's Build button.
pub(crate) fn palette_open(world: &World) -> bool {
    overseeing(world)
        && world
            .get_resource::<BuildTool>()
            .is_some_and(|tool| tool.open)
}

/// The site or building the card is about, while overseeing.
pub(crate) fn card_subject(world: &World) -> Option<SimId> {
    if !overseeing(world) {
        return None;
    }
    let tool = world.get_resource::<BuildTool>()?;
    // The right column holds one thing: a ledger page hides the card.
    if tool.open || (crate::island_shell::enabled(world) && crate::colony_panel::open(world)) {
        return None;
    }
    tool.card.filter(|id| {
        world
            .get_resource::<SimIndex>()
            .is_some_and(|index| index.get(*id).is_some())
    })
}

/// Whether the build menu or a building card is being read. The alert stack
/// folds to its header and the marker key hides while it is.
/// Buildings ordered that time has not opened yet (the Paused pill's count).
pub(crate) fn orders_waiting(world: &World) -> usize {
    world
        .get_resource::<BuildTool>()
        .map_or(0, BuildTool::queued)
}

pub(crate) fn column_taken(world: &World) -> bool {
    palette_open(world) || card_subject(world).is_some()
}

/// Give the column back: close the palette, the tool and the card.
pub(crate) fn close_column(world: &mut World) {
    if let Some(mut tool) = world.get_resource_mut::<BuildTool>() {
        tool.open = false;
        tool.selected = None;
        tool.ghost = None;
        tool.ghost_key = None;
        tool.card = None;
    }
}

/// A category of the build dock: open the build menu on it, or close the
/// menu when it already shows that category.
pub(crate) fn request_group(world: &mut World, group: Group) {
    let shown = open_group(world);
    if let Some(mut tool) = world.get_resource_mut::<BuildTool>() {
        if shown == Some(group) {
            tool.requests.push(Action::Close);
        } else {
            if !tool.open {
                tool.requests.push(Action::Toggle);
            }
            tool.requests.push(Action::Group(group));
        }
    }
}

/// The categories that have something to build, in dock order. An empty
/// category is left out, not disabled.
pub(crate) fn groups(world: &World) -> Vec<Group> {
    world
        .get_resource::<BuildTool>()
        .map(|tool| {
            model::grouped(&tool.items)
                .into_iter()
                .map(|(group, _)| group)
                .collect()
        })
        .unwrap_or_default()
}

/// The category the open build menu shows.
pub(crate) fn open_group(world: &World) -> Option<Group> {
    if !palette_open(world) {
        return None;
    }
    let tool = world.get_resource::<BuildTool>()?;
    model::shown_group(&tool.items, tool.group)
}

/// The site or building whose card is asked for, whatever else is open.
pub(crate) fn card_raw(world: &World) -> Option<SimId> {
    world.get_resource::<BuildTool>().and_then(|tool| tool.card)
}

/// Close the building card and leave the build menu as it is.
pub(crate) fn close_card(world: &mut World) {
    if let Some(mut tool) = world.get_resource_mut::<BuildTool>() {
        tool.card = None;
    }
}

pub(crate) fn good_name(world: &World, kind: &str) -> String {
    catalog::display_in(world, kind).map_or_else(
        || {
            kind.strip_prefix("village_")
                .unwrap_or(kind)
                .replace('_', " ")
        },
        str::to_owned,
    )
}

fn settlement_label(world: &World, id: SimId) -> Option<String> {
    let entity = world.get_resource::<SimIndex>()?.get(id)?;
    world
        .get::<frontier::Settlement>(entity)
        .map(|place| place.label.clone())
}

fn person_name(world: &World, id: SimId) -> Option<String> {
    let entity = world.get_resource::<SimIndex>()?.get(id)?;
    world
        .get::<Villager>(entity)
        .map(|person| person.name.clone())
}

/// The label of a site, a finished building, a home or a workplace.
pub(crate) fn place_label(world: &World, id: SimId) -> Option<String> {
    let entity = world.get_resource::<SimIndex>()?.get(id)?;
    world
        .get::<civic::ConstructionSite>(entity)
        .map(|site| site.label.clone())
        .or_else(|| {
            world
                .get::<civic::Building>(entity)
                .map(|building| building.label.clone())
        })
        .or_else(|| {
            world
                .get::<Household>(entity)
                .map(|home| home.label.clone())
        })
        .or_else(|| {
            world
                .get::<VillagePlace>(entity)
                .map(|place| place.label.clone())
        })
}

/// The palette's buildings for the running world.
pub(crate) fn items(world: &World) -> Vec<PaletteItem> {
    let mut items = model::palette(civic::catalog(world), &|kind| good_name(world, kind));
    // The simulation's own availability: a building the fund and the levy
    // cannot pay for, or one that needs a good nobody trades, says why.
    let catalogue = island_content::village::view::catalogue(world);
    for item in &mut items {
        item.unavailable = catalogue
            .iter()
            .find(|entry| entry.building == item.id && !entry.available)
            .map(|entry| entry.reason.clone());
    }
    items
}

fn day_ticks(world: &World) -> u64 {
    world
        .get_resource::<SimConfig>()
        .map_or(2400, |config| config.day_length)
}

fn site_row(world: &World, site: &civic::SiteView) -> SiteRow {
    let now = world
        .get_resource::<SimClock>()
        .map_or(0, |clock| clock.tick);
    let placed = world
        .get_resource::<SimIndex>()
        .and_then(|index| index.get(site.id))
        .and_then(|entity| world.get::<civic::ConstructionSite>(entity))
        .map_or(now, |state| state.placed_at);
    let since = world
        .get_resource::<CivicLog>()
        .and_then(|log| log.watch.get(&site.id))
        .map_or(placed, |(_, since)| *since);
    let materials: model::Materials = site
        .cost
        .iter()
        .map(|(kind, needed)| {
            (
                good_name(world, kind),
                site.delivered.get(kind).copied().unwrap_or(0),
                *needed,
            )
        })
        .collect();
    let supplied = materials
        .iter()
        .all(|(_, delivered, needed)| delivered >= needed);
    SiteRow {
        id: site.id.0,
        label: site.label.clone(),
        materials,
        work_done: site.work_done,
        work_ticks: site.work_ticks,
        civic: site.civic,
        reason: site.reason.clone(),
        builder: site.builder.and_then(|id| person_name(world, id)),
        waiting_days: model::waiting_days(now, since, day_ticks(world), supplied),
        unfunded: !supplied && civic::budget(world) == 0,
    }
}

/// Every open site, ascending by identity, with where its entrance is.
pub(crate) fn site_rows(world: &World) -> Vec<(SiteRow, SimPos)> {
    civic::sites(world)
        .iter()
        .map(|site| (site_row(world, site), site.door))
        .collect()
}

/// What the projects panel shows above its sites.
pub(crate) fn fund_line(world: &World) -> String {
    model::fund_line(
        world
            .get_resource::<village::VillageState>()
            .map_or(0, |state| state.fund),
        civic::budget(world),
        civic::fund_hold(world),
        civic::sites(world).len(),
    )
}

/// One line per settlement on what its planner saw and decided last.
pub(crate) fn planner_lines(world: &World) -> Vec<(SimId, String)> {
    if !available(world) {
        return Vec::new();
    }
    frontier::settlements::all(world)
        .into_iter()
        .filter_map(|(id, entity)| {
            let place = world.get::<frontier::Settlement>(entity)?;
            let planner = civic::planner(world, id)?;
            // The planner remembers "commissioned <key>": name the building.
            let decision = model::last_decision(&planner.last_decision, &|key| {
                civic::definition(world, key).map(|def| def.display.clone())
            });
            Some((
                id,
                model::planner_line(
                    &place.label,
                    planner.homeless,
                    planner.free_beds,
                    planner.surplus_units,
                    &decision,
                ),
            ))
        })
        .collect()
}

/// The newest planner decisions of this session, newest first.
pub(crate) fn recent_decisions(world: &World, limit: usize) -> Vec<Decision> {
    world
        .get_resource::<CivicLog>()
        .map(|log| {
            model::recent(&log.decisions, limit)
                .into_iter()
                .cloned()
                .collect()
        })
        .unwrap_or_default()
}

/// Bring the camera to a site or a building and open its card.
pub(crate) fn focus_place(world: &mut World, id: SimId) {
    let Some(position) = world
        .get_resource::<SimIndex>()
        .and_then(|index| index.get(id))
        .and_then(|entity| world.get::<SimPos>(entity))
        .copied()
    else {
        return;
    };
    crate::resident_inspector::close(world);
    if let Some(mut tool) = world.get_resource_mut::<BuildTool>() {
        tool.open = false;
        tool.selected = None;
        tool.ghost = None;
        tool.ghost_key = None;
        tool.card = Some(id);
    }
    crate::island_camera::focus(world, sim_to_world(position), FOCUS_DISTANCE);
}

/// The card for a site, a finished building, a home or a workplace.
pub(crate) fn card(world: &World, id: SimId) -> Option<Card> {
    let entity = world.get_resource::<SimIndex>()?.get(id)?;
    let settlement = frontier::settlements::member(world, entity)
        .and_then(|settlement| settlement_label(world, settlement));
    let upkeep = civic::upkeep::view(world, id).map(|view| model::CardUpkeep {
        condition: view.condition,
        needs_repair: view.needs_repair,
        worn: view.worn,
        unusable: view.unusable,
        burning: view.burning,
        repairing: view.repair_site.is_some(),
    });
    if world.get::<civic::ConstructionSite>(entity).is_some() {
        let site = civic::sites(world).into_iter().find(|site| site.id == id)?;
        return Some(Card {
            title: site.label.clone(),
            settlement,
            upkeep,
            body: CardBody::Site(site_row(world, &site)),
        });
    }
    let built = world.get::<civic::Building>(entity);
    if let Some(depot) = world.get::<civic::Depot>(entity) {
        return Some(Card {
            title: built.map_or_else(|| "Store".to_owned(), |building| building.label.clone()),
            settlement,
            upkeep,
            body: CardBody::Depot {
                stall: depot.public,
                shelf: Shelf {
                    used: depot.used(),
                    capacity: depot.capacity,
                    stock: depot
                        .lots
                        .keys()
                        .map(|kind| (good_name(world, kind), depot.stock(kind)))
                        .collect(),
                    sold_units: depot.sold_units,
                    turnover: depot.turnover,
                },
            },
        });
    }
    if let Some(home) = world.get::<Household>(entity) {
        let now = world
            .get_resource::<SimClock>()
            .map_or(0, |clock| clock.tick);
        return Some(Card {
            title: home.label.clone(),
            settlement,
            upkeep,
            body: CardBody::Home {
                beds: home.capacity,
                residents: home
                    .residents
                    .iter()
                    .filter_map(|resident| person_name(world, *resident))
                    .collect(),
                heated: home.heated_until > now,
            },
        });
    }
    let place = world.get::<VillagePlace>(entity)?;
    let workers = world
        .get_resource::<SimIndex>()?
        .map
        .values()
        .filter_map(|person| {
            world
                .get::<Villager>(*person)
                .filter(|worker| worker.workplace == id && village::living(world, *person))
                .map(|worker| worker.name.clone())
        })
        .collect();
    Some(Card {
        title: model::workplace_title(
            &place.label,
            place.role.name(),
            built.map(|building| building.label.as_str()),
        ),
        settlement,
        upkeep,
        body: CardBody::Workplace {
            trade: crate::colony_panel::trade_display(world, place.role),
            seats: civic::workplace_seats(world, entity),
            workers,
            // A service trade's house (the inn) takes guests: who is inside
            // now, from the simulation's own list of occupants.
            inside: village::frontier::economy_spec::is_service(world, place.role).then(|| {
                village::view::occupants(world, id)
                    .into_iter()
                    .filter_map(|guest| {
                        let entity = world.get_resource::<SimIndex>()?.get(guest)?;
                        Some(world.get::<Villager>(entity)?.name.clone())
                    })
                    .collect()
            }),
            // The inn's guest beds, from the simulation's own view of it:
            // who has rented each and until what time of day.
            guest_beds: village::view::inn(world, id).map_or_else(Vec::new, |inn| {
                let day_length = world
                    .get_resource::<npc_framework::prelude::SimConfig>()
                    .map_or(2400, |config| config.day_length)
                    .max(1);
                inn.beds
                    .iter()
                    .map(|bed| {
                        bed.guest.map(|_| {
                            let until = if bed.until == 0 {
                                String::new()
                            } else {
                                let minutes = (bed.until % day_length) * 1440 / day_length;
                                format!("{:02}:{:02}", minutes / 60, minutes % 60)
                            };
                            (bed.name.clone(), until)
                        })
                    })
                    .collect()
            }),
        },
    })
}

/// What a resident on a building errand is doing, with the real names:
/// "building Harbour cottage 2", "carrying planks to Harbour cottage 2",
/// "selling bread at Harbour market stall 1". `leaf` is the running leaf's
/// own label; `None` when the resident's stated job does not belong to it.
pub(crate) fn errand(world: &World, entity: Entity, leaf: &str) -> Option<String> {
    let job = world.get::<JobIntent>(entity)?;
    if !model::errand_verbs(leaf).contains(&job.verb.as_str()) {
        return None;
    }
    let target = job.target.and_then(|id| place_label(world, id));
    let good = good_name(world, &job.good);
    if let Some(label) = model::errand_label(&job.verb, &good, target.as_deref()) {
        return Some(label);
    }
    // A sale or purchase counts as a shelf trade only at a store or a stall.
    let shelf = job
        .target
        .and_then(|id| world.get_resource::<SimIndex>()?.get(id))
        .filter(|shelf| world.get::<civic::Depot>(*shelf).is_some())?;
    let label = world
        .get::<civic::Building>(shelf)
        .map(|building| building.label.clone())?;
    model::shelf_trade_label(&job.verb, &good, &label)
}

/// The site a resident is building on right now, by its label.
pub(crate) fn building_site(world: &World, entity: Entity) -> Option<String> {
    let work = world.get::<civic::SiteWork>(entity)?;
    place_label(world, work.site)
}

fn typing(world: &World) -> bool {
    world
        .get_resource::<bevy::input_focus::InputFocus>()
        .and_then(|focus| focus.get())
        .is_some_and(|entity| world.get::<bevy::text::EditableText>(entity).is_some())
}

/// B opens the palette; with a building in hand R turns it and Esc puts it
/// down; Esc then closes the palette, or the building card. Handled keys are
/// consumed, so Esc does not also open the pause menu.
pub(crate) fn keys(world: &mut World) {
    if !overseeing(world) || !crate::island_shell::enabled(world) || typing(world) {
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
    let tool = world.resource::<BuildTool>();
    let (holding, open, card) = (tool.selected.is_some(), tool.open, tool.card.is_some());
    let mut pressed = Vec::new();
    let tray = crate::keymap::key(crate::keymap::Mode::Oversee, crate::keymap::BUILD_TRAY);
    if keys.just_pressed(tray) {
        pressed.push((tray, Action::Toggle));
    }
    if holding && keys.just_pressed(KeyCode::KeyR) {
        pressed.push((KeyCode::KeyR, Action::Rotate));
    }
    if keys.just_pressed(KeyCode::Escape) {
        if holding {
            pressed.push((KeyCode::Escape, Action::CancelTool));
        } else if open {
            pressed.push((KeyCode::Escape, Action::Close));
        } else if card {
            pressed.push((KeyCode::Escape, Action::CloseCard));
        }
    }
    if pressed.is_empty() {
        return;
    }
    let mut keys = world.resource_mut::<ButtonInput<KeyCode>>();
    for (key, _) in &pressed {
        keys.reset(*key);
    }
    world
        .resource_mut::<BuildTool>()
        .requests
        .extend(pressed.into_iter().map(|(_, action)| action));
}

fn cursor_ray(world: &mut World) -> Option<Ray3d> {
    let cursor = world
        .query::<&Window>()
        .iter(world)
        .find_map(Window::cursor_position)?;
    world
        .query_filtered::<(&Camera, &GlobalTransform), With<crate::RtsCamera>>()
        .iter(world)
        .find_map(|(camera, transform)| camera.viewport_to_world(transform, cursor).ok())
}

/// With a building in hand the pointer aims the ghost: a left click places,
/// a right click puts the building down. Without one, a left click on the
/// world is remembered so `process` can open a building card, once resident
/// selection has had its say.
pub(crate) fn pointer(world: &mut World) {
    if !overseeing(world) {
        let mut tool = world.resource_mut::<BuildTool>();
        tool.hover = None;
        tool.click = None;
        return;
    }
    let ui_hovered = world
        .query_filtered::<&Hovered, With<Node>>()
        .iter(world)
        .any(Hovered::get);
    let holding = world.resource::<BuildTool>().selected.is_some();
    if ui_hovered {
        world.resource_mut::<BuildTool>().hover = None;
        return;
    }
    let (left, right) =
        world
            .get_resource::<ButtonInput<MouseButton>>()
            .map_or((false, false), |mouse| {
                (
                    mouse.just_pressed(MouseButton::Left),
                    mouse.just_pressed(MouseButton::Right),
                )
            });
    if !holding {
        world.resource_mut::<BuildTool>().hover = None;
        if left {
            let ray = cursor_ray(world);
            world.resource_mut::<BuildTool>().click = ray;
        }
        return;
    }
    let cell = crate::shape_tools::picking::cursor_cell(world)
        .filter(|(x, z)| crate::shape_tools::picking::terrain_cell(world, *x, *z).is_some());
    let mut tool = world.resource_mut::<BuildTool>();
    tool.hover = cell;
    if right {
        tool.requests.push(Action::CancelTool);
    } else if left && cell.is_some() {
        tool.requests.push(Action::Place);
    }
    // Placing must never also select a resident or issue an order.
    if let Some(mut mouse) = world.get_resource_mut::<ButtonInput<MouseButton>>() {
        for button in [MouseButton::Left, MouseButton::Right] {
            mouse.clear_just_pressed(button);
            mouse.clear_just_released(button);
        }
    }
    if let Some(mut drag) = world.get_resource_mut::<DragBox>() {
        drag.start = None;
    }
}

/// Read new planner decisions from the journal and note deliveries.
pub(crate) fn track(world: &mut World) {
    let epoch = npc_framework::agent_io::api::epoch(world);
    if world.resource::<CivicLog>().epoch != Some(epoch) {
        *world.resource_mut::<CivicLog>() = CivicLog {
            epoch: Some(epoch),
            elapsed: WATCH_SECS,
            ..default()
        };
    }
    harvest(world);
    let delta = world.get_resource::<Time>().map_or(0.0, Time::delta_secs);
    {
        let mut log = world.resource_mut::<CivicLog>();
        log.elapsed += delta;
        if log.elapsed < WATCH_SECS {
            return;
        }
        log.elapsed = 0.0;
    }
    watch(world);
}

/// When each open site last received anything, as seen by this session. A
/// site that was already open counts from the day it was placed.
fn watch(world: &mut World) {
    let now = world
        .get_resource::<SimClock>()
        .map_or(0, |clock| clock.tick);
    let seen: Vec<(SimId, u32, u64)> = civic::sites(world)
        .iter()
        .map(|site| {
            let placed = world
                .get_resource::<SimIndex>()
                .and_then(|index| index.get(site.id))
                .and_then(|entity| world.get::<civic::ConstructionSite>(entity))
                .map_or(now, |state| state.placed_at);
            (
                site.id,
                site.delivered
                    .values()
                    .copied()
                    .fold(0, u32::saturating_add),
                placed,
            )
        })
        .collect();
    let mut log = world.resource_mut::<CivicLog>();
    let before = log.watch.clone();
    log.watch
        .retain(|id, _| seen.iter().any(|(site, _, _)| site == id));
    for (id, delivered, placed) in seen {
        match log.watch.get_mut(&id) {
            Some(entry) if entry.0 != delivered => *entry = (delivered, now),
            Some(_) => {}
            None => {
                log.watch.insert(id, (delivered, placed));
            }
        }
    }
    if log.watch != before {
        log.revision = log.revision.wrapping_add(1);
    }
}

/// Fold new `VillageCivicProject` effects into the decision log. The journal
/// is a bounded ring, so one pass over its unread tail is bounded too.
fn harvest(world: &mut World) {
    let cursor = world.resource::<CivicLog>().cursor;
    let Some(journal) = world.get_resource::<Journal>() else {
        return;
    };
    let latest = journal.entries.back().map(|entry| (entry.tick, entry.seq));
    if latest == cursor {
        return;
    }
    // A journal that moved backwards belongs to another timeline.
    let rewound = matches!((latest, cursor), (Some(latest), Some(cursor)) if latest < cursor);
    let mut fresh: Vec<(u64, Value)> = journal
        .entries
        .iter()
        .rev()
        .take_while(|entry| rewound || cursor.is_none_or(|cursor| (entry.tick, entry.seq) > cursor))
        .filter_map(|entry| match &entry.kind {
            JournalKind::Effect {
                effect,
                applied: true,
            } if effect.get("type").and_then(Value::as_str) == Some("VillageCivicProject") => {
                Some((entry.tick, serde_json::to_value(effect).ok()?))
            }
            _ => None,
        })
        .collect();
    fresh.reverse();
    let decisions: Vec<Decision> = fresh
        .into_iter()
        .map(|(tick, payload)| decision(world, tick, &payload))
        .collect();
    let mut log = world.resource_mut::<CivicLog>();
    if rewound {
        log.decisions.clear();
    }
    log.cursor = latest;
    if decisions.is_empty() && !rewound {
        return;
    }
    log.decisions.extend(decisions);
    let excess = log.decisions.len().saturating_sub(LOG_CAP);
    log.decisions.drain(..excess);
    log.revision = log.revision.wrapping_add(1);
}

fn decision(world: &World, tick: u64, payload: &Value) -> Decision {
    let id = |key: &str| {
        payload
            .get(key)
            .and_then(Value::as_u64)
            .and_then(|id| u32::try_from(id).ok())
    };
    let settlement = id("settlement")
        .and_then(|id| settlement_label(world, SimId(id)))
        .unwrap_or_else(|| "The village".into());
    let building = payload.get("building").and_then(Value::as_str).map(|key| {
        civic::definition(world, key)
            .map_or_else(|| key.replace('_', " "), |def| def.display.clone())
    });
    Decision {
        tick,
        site: id("site"),
        text: model::decision_text(&settlement, building.as_deref(), payload),
    }
}

/// What can be clicked to open a card: `(id, box low corner, box high corner)`.
fn targets(world: &World) -> Vec<(SimId, Vec3, Vec3)> {
    let Some(index) = world.get_resource::<SimIndex>() else {
        return Vec::new();
    };
    let ground = |x: f32, z: f32| {
        crate::shape_tools::picking::terrain_cell(world, x.floor() as i32, z.floor() as i32)
            .map_or(0.0, |cell| cell.height_mm.max(0) as f32 / 1000.0)
    };
    let mut found = Vec::new();
    for (id, entity) in &index.map {
        // A home or an inn is as big as its model: the body the simulation
        // gives it, up to the chimney. A click on a roof or an outer wall
        // selects the house.
        if let Some((lo, hi, height)) = crate::island_homes::body_box(world, *entity, *id) {
            let y = ground((lo.x + hi.x) * 0.5, (lo.y + hi.y) * 0.5);
            found.push((
                *id,
                Vec3::new(lo.x, y - 0.5, lo.y),
                Vec3::new(hi.x, y + height, hi.y),
            ));
            continue;
        }
        let plot = world
            .get::<civic::ConstructionSite>(*entity)
            .map(|site| (site.plot, 1.4))
            .or_else(|| {
                world
                    .get::<civic::Building>(*entity)
                    .map(|building| (building.plot, if building.plot.solid { 3.4 } else { 1.6 }))
            });
        if let Some((plot, height)) = plot {
            let y = ground(plot.x as f32 + 0.5, plot.z as f32 + 0.5);
            found.push((
                *id,
                Vec3::new(plot.lo_x as f32, y - 0.5, plot.lo_z as f32),
                Vec3::new(plot.hi_x as f32 + 1.0, y + height, plot.hi_z as f32 + 1.0),
            ));
            continue;
        }
        let Some(position) = world.get::<SimPos>(*entity) else {
            continue;
        };
        // Founding workplaces stand at a fixed offset from their own position.
        let (centre_z, half) = if world.get::<VillagePlace>(*entity).is_some() {
            (position.z + WORKPLACE_OFFSET_MM, Vec2::new(2.4, 2.4))
        } else {
            continue;
        };
        let (x, z) = (position.x as f32 / 1000.0, centre_z as f32 / 1000.0);
        let y = ground(x, z);
        found.push((
            *id,
            Vec3::new(x - half.x, y - 0.5, z - half.y),
            Vec3::new(x + half.x, y + 3.4, z + half.y),
        ));
    }
    found
}

/// The building or home under the pointer, in any mode (Live opens your own
/// cottage this way).
pub(crate) fn building_at_cursor(world: &mut World) -> Option<SimId> {
    let ray = cursor_ray(world)?;
    pick(world, ray)
}

/// The building or site the ray meets first.
fn pick(world: &World, ray: Ray3d) -> Option<SimId> {
    let origin = ray.origin.to_array();
    let dir = (*ray.direction).to_array();
    targets(world)
        .into_iter()
        .filter_map(|(id, lo, hi)| {
            model::ray_box(origin, dir, lo.to_array(), hi.to_array()).map(|at| (at, id))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)))
        .map(|(_, id)| id)
}

fn terrain_revision(world: &World) -> u64 {
    crate::shape_tools::picking::revision(world)
}

/// Ask the simulation about the ghost when it moved, turned or changed, and
/// every half second otherwise.
fn refresh_ghost(world: &mut World, delta: f32) {
    let (building, cell, rotation) = {
        let tool = world.resource::<BuildTool>();
        (tool.selected.clone(), tool.hover, tool.rotation)
    };
    let (Some(building), Some(cell)) = (building, cell) else {
        let mut tool = world.resource_mut::<BuildTool>();
        tool.ghost = None;
        tool.ghost_key = None;
        return;
    };
    let key = GhostKey {
        building: building.clone(),
        cell,
        rotation,
        revision: terrain_revision(world),
    };
    {
        let mut tool = world.resource_mut::<BuildTool>();
        tool.since_check += delta;
        if tool.ghost_key.as_ref() == Some(&key) && tool.since_check < RECHECK_SECS {
            return;
        }
        tool.since_check = 0.0;
    }
    let plot = civic::placement_plot(world, &building, cell.0, cell.1, rotation);
    let check = civic::placement_check(world, &building, cell.0, cell.1, rotation);
    let ghost = Ghost::new(&building, cell, rotation, plot, check);
    let mut tool = world.resource_mut::<BuildTool>();
    tool.ghost = Some(ghost);
    tool.ghost_key = Some(key);
}

fn name_of(tool: &BuildTool, building: &str) -> String {
    tool.items
        .iter()
        .find(|item| item.id == building)
        .map_or_else(|| building.replace('_', " "), |item| item.name.clone())
}

fn apply(world: &mut World, action: Action) {
    match action {
        Action::Toggle => {
            let open = !world.resource::<BuildTool>().open;
            if open {
                // The tray sits over the lower part of the right column:
                // opening it closes whatever that column held.
                crate::colony_panel::close(world);
                crate::resident_inspector::close(world);
            }
            let mut tool = world.resource_mut::<BuildTool>();
            tool.open = open;
            tool.card = None;
            if !open {
                tool.selected = None;
            }
        }
        Action::Close => {
            let mut tool = world.resource_mut::<BuildTool>();
            tool.open = false;
            tool.selected = None;
        }
        Action::Group(group) => world.resource_mut::<BuildTool>().group = group,
        Action::Pick(building) => {
            let mut tool = world.resource_mut::<BuildTool>();
            let refused = tool
                .items
                .iter()
                .find(|item| item.id == building)
                .and_then(|item| item.unavailable.clone());
            if let Some(reason) = refused {
                // Nothing is taken in hand; the card and this line say why.
                tool.status = reason;
            } else if tool.selected.as_deref() == Some(building.as_str()) {
                tool.selected = None;
            } else {
                tool.status = format!("{} in hand.", name_of(&tool, &building));
                tool.selected = Some(building);
                tool.rejection = None;
            }
            tool.ghost_key = None;
        }
        Action::Rotate => {
            let mut tool = world.resource_mut::<BuildTool>();
            tool.rotation = model::rotated(tool.rotation);
            tool.ghost_key = None;
        }
        Action::CancelTool => {
            let mut tool = world.resource_mut::<BuildTool>();
            tool.selected = None;
            tool.status = String::new();
        }
        Action::Place => place(world),
        Action::CloseCard => world.resource_mut::<BuildTool>().card = None,
        Action::CardTab(tab) => world.resource_mut::<BuildTool>().card_tab = tab,
        Action::FocusCard => {
            if let Some(id) = world.resource::<BuildTool>().card {
                let position = world
                    .get_resource::<SimIndex>()
                    .and_then(|index| index.get(id))
                    .and_then(|entity| world.get::<SimPos>(entity))
                    .copied();
                if let Some(position) = position {
                    crate::island_camera::focus(world, sim_to_world(position), FOCUS_DISTANCE);
                }
            }
        }
    }
}

/// Record one placement where the ghost stands, if the simulation agrees.
fn place(world: &mut World) {
    // The click acts on what the simulation says now, not half a second ago.
    world.resource_mut::<BuildTool>().ghost_key = None;
    refresh_ghost(world, 0.0);
    let Some(ghost) = world.resource::<BuildTool>().ghost.clone() else {
        return;
    };
    let name = name_of(world.resource::<BuildTool>(), &ghost.building);
    if let Some(reason) = &ghost.blocked {
        let mut tool = world.resource_mut::<BuildTool>();
        tool.status = format!("{name} not placed: {reason}");
        crate::game_ui::sound::play(world, crate::game_ui::sound::UiSound::Deny);
        return;
    }
    let command = SimCommand::content(&civic::PlaceBuilding {
        building: ghost.building.clone(),
        x: ghost.cell.0,
        z: ghost.cell.1,
        rotation: ghost.rotation,
    });
    // Deferred validation: the tick that applies the command checks it
    // against the world of that moment, and `settle` reports a refusal.
    match submit_deferred(world, None, command) {
        Ok(key) => {
            let stopped = world
                .get_resource::<SimControl>()
                .is_some_and(SimControl::is_stopped);
            let mut tool = world.resource_mut::<BuildTool>();
            tool.inflight.push((key, name.clone()));
            tool.rejection = None;
            tool.status = if stopped {
                format!("{name} is queued. The site opens when time runs (Space).")
            } else {
                format!("{name} ordered.")
            };
        }
        Err(error) => {
            let mut tool = world.resource_mut::<BuildTool>();
            tool.status = format!("{name} not placed: {error}");
            tool.rejection = Some((error.to_string(), REJECTION_SECS));
            crate::game_ui::sound::play(world, crate::game_ui::sound::UiSound::Deny);
        }
    }
}

/// Match recorded placements with the simulation's own outcome.
fn settle(world: &mut World) {
    if world.resource::<BuildTool>().inflight.is_empty() {
        return;
    }
    let pending: Vec<_> = world
        .resource::<BuildTool>()
        .inflight
        .iter()
        .map(|(key, name)| {
            let waiting = world
                .get_resource::<CommandQueue>()
                .is_some_and(|queue| queue.pending.contains_key(key));
            (*key, name.clone(), waiting)
        })
        .collect();
    let first_tick = pending.iter().map(|(key, _, _)| key.0).min().unwrap_or(0);
    let skipped: BTreeMap<(u64, u32), String> = world
        .get_resource::<SkippedCommands>()
        .map(|log| {
            log.entries
                .iter()
                .rev()
                .take_while(|entry| entry.tick >= first_tick)
                .map(|entry| ((entry.tick, entry.seq), entry.error.to_string()))
                .collect()
        })
        .unwrap_or_default();
    let mut sounds = Vec::new();
    let mut tool = world.resource_mut::<BuildTool>();
    tool.inflight.clear();
    for (key, name, waiting) in pending {
        if waiting {
            tool.inflight.push((key, name));
        } else if let Some(reason) = skipped.get(&key) {
            tool.status = format!("{name} not placed: {reason}");
            tool.rejection = Some((reason.clone(), REJECTION_SECS));
            sounds.push(crate::game_ui::sound::UiSound::Deny);
        } else {
            tool.status =
                format!("{name}: site opened. Residents bring the materials, then build.");
            // The next check must see the new site under the ghost.
            tool.ghost_key = None;
            sounds.push(crate::game_ui::sound::UiSound::Place);
        }
    }
    // Once, when the simulation has answered: placed or refused.
    for sound in sounds {
        crate::game_ui::sound::play(world, sound);
    }
}

pub(crate) fn process(world: &mut World) {
    let epoch = npc_framework::agent_io::api::epoch(world);
    if world.resource::<BuildTool>().epoch != Some(epoch) {
        // A different world: nothing in hand or in flight belongs to it.
        let mut tool = world.resource_mut::<BuildTool>();
        let (open, group, requests) = (tool.open, tool.group, std::mem::take(&mut tool.requests));
        *tool = BuildTool {
            open,
            group,
            requests,
            epoch: Some(epoch),
            ..default()
        };
    }
    if world.resource::<BuildTool>().items_epoch != Some(epoch) {
        let items = items(world);
        let mut tool = world.resource_mut::<BuildTool>();
        tool.items = items;
        tool.items_epoch = Some(epoch);
    }
    let requests = std::mem::take(&mut world.resource_mut::<BuildTool>().requests);
    let active = overseeing(world);
    for request in requests {
        if active {
            apply(world, request);
        }
    }
    if !active {
        // Leaving Oversee puts the building down; the palette choice stays.
        let mut tool = world.resource_mut::<BuildTool>();
        tool.selected = None;
        tool.ghost = None;
        tool.ghost_key = None;
        tool.click = None;
    }
    // A resident was chosen: the column is theirs.
    if crate::resident_inspector::open(world) {
        let mut tool = world.resource_mut::<BuildTool>();
        if tool.open || tool.card.is_some() {
            tool.open = false;
            tool.selected = None;
            tool.card = None;
        }
        tool.click = None;
    }
    if let Some(ray) = world.resource_mut::<BuildTool>().click.take() {
        let picked = pick(world, ray);
        let mut tool = world.resource_mut::<BuildTool>();
        if !tool.open {
            tool.card = picked;
        }
    }
    // The card's subject may have been cancelled or replaced by another world.
    if let Some(id) = world.resource::<BuildTool>().card
        && card(world, id).is_none()
    {
        world.resource_mut::<BuildTool>().card = None;
    }
    let delta = world.get_resource::<Time>().map_or(0.0, Time::delta_secs);
    refresh_ghost(world, delta);
    settle(world);
    let mut tool = world.resource_mut::<BuildTool>();
    if let Some((_, left)) = tool.rejection.as_mut() {
        *left -= delta;
    }
    if tool
        .rejection
        .as_ref()
        .is_some_and(|(_, left)| *left <= 0.0)
    {
        tool.rejection = None;
    }
    outline(world);
}

// The verdict is never colour alone: the tag beside the cursor says it.
const VALID: Color = crate::game_ui::tokens::GOOD_LIGHT;
const BLOCKED: Color = crate::game_ui::tokens::CRIT_LIGHT;

fn cell_top(world: &World, x: i32, z: i32) -> f32 {
    crate::shape_tools::picking::terrain_cell(world, x, z)
        .map_or(0.0, |cell| cell.height_mm.max(0) as f32 / 1000.0)
}

/// The ghost's cells and entrance, and the outline of the card's subject.
fn outline(world: &mut World) {
    let mut cubes: Vec<(Vec3, Vec3, Color)> = Vec::new();
    let mut lines: Vec<(Vec3, Vec3, Color)> = Vec::new();
    let tool = world.resource::<BuildTool>();
    if !overseeing(world) {
        if !tool.cubes.is_empty() || !tool.lines.is_empty() {
            let mut tool = world.resource_mut::<BuildTool>();
            tool.cubes.clear();
            tool.lines.clear();
        }
        return;
    }
    if let Some(ghost) = &tool.ghost {
        let color = match ghost.tint() {
            Tint::Valid => VALID,
            Tint::Blocked => BLOCKED,
        };
        let (body, entrance) = ghost.cells();
        for (x, z) in body {
            cubes.push((
                Vec3::new(x as f32 + 0.5, cell_top(world, x, z) + 0.05, z as f32 + 0.5),
                Vec3::new(1.0, 0.06, 1.0),
                color,
            ));
        }
        // The entrance: a gold cell and a post where the door will be.
        let at = Vec3::new(
            entrance.0 as f32 + 0.5,
            cell_top(world, entrance.0, entrance.1) + 0.05,
            entrance.1 as f32 + 0.5,
        );
        cubes.push((at, Vec3::new(0.9, 0.06, 0.9), ui::GOLD));
        if let Some(plot) = ghost.plot {
            let centre = Vec3::new(
                (plot.lo_x + plot.hi_x + 1) as f32 * 0.5,
                at.y,
                (plot.lo_z + plot.hi_z + 1) as f32 * 0.5,
            );
            lines.push((at, centre, ui::GOLD));
        }
    }
    if let Some(id) = card_subject(world)
        && let Some(entity) = world.resource::<SimIndex>().get(id)
    {
        let plot = world
            .get::<civic::ConstructionSite>(entity)
            .map(|site| site.plot)
            .or_else(|| world.get::<civic::Building>(entity).map(|built| built.plot));
        if let Some(plot) = plot {
            let y = cell_top(world, plot.x, plot.z) + 0.08;
            let (lo_x, lo_z) = (plot.lo_x as f32, plot.lo_z as f32);
            let (hi_x, hi_z) = (plot.hi_x as f32 + 1.0, plot.hi_z as f32 + 1.0);
            let corners = [
                Vec3::new(lo_x, y, lo_z),
                Vec3::new(hi_x, y, lo_z),
                Vec3::new(hi_x, y, hi_z),
                Vec3::new(lo_x, y, hi_z),
            ];
            for at in 0..4 {
                lines.push((corners[at], corners[(at + 1) % 4], ui::GOLD));
            }
        } else if let Some(position) = world.get::<SimPos>(entity) {
            let at = sim_to_world(*position);
            let at = Vec3::new(
                at.x,
                cell_top(world, at.x.floor() as i32, at.z.floor() as i32),
                at.z,
            );
            cubes.push((at + Vec3::Y * 0.05, Vec3::new(1.0, 0.06, 1.0), ui::GOLD));
        }
    }
    let mut tool = world.resource_mut::<BuildTool>();
    if tool.cubes != cubes {
        tool.cubes = cubes;
    }
    if tool.lines != lines {
        tool.lines = lines;
    }
}

fn gizmo(tool: Res<BuildTool>, mut gizmos: Gizmos) {
    for (centre, size, color) in &tool.cubes {
        gizmos.cube(
            Transform::from_translation(*centre).with_scale(*size),
            *color,
        );
    }
    for (from, to, color) in &tool.lines {
        gizmos.line(*from, *to, *color);
    }
}
