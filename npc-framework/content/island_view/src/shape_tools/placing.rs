//! Shape's plots and props: lay out farm and garden plots and set props, and
//! take placed ones away.
//!
//! The Place tile (L) opens a panel built from the world's catalog: every
//! building with a `field` is a plot, every entry of `economy.props` a prop,
//! so a new entry in data needs no code here. Choosing one takes it up; R
//! turns it a quarter, the click places it, and "Remove placed" takes a
//! placed plot or prop away. Each is a recorded command (`PlacePlot`,
//! `PlaceProp`, `RemovePlot`, `RemoveProp`); the simulation's own check is
//! the verdict beside the cursor and the colour of the ghost before the
//! click, and its refusal is the notice after it.
//!
//! Unlike resource patches, placements take part in Undo and Redo: the
//! inverse of a placement is the removal of the identity the simulation
//! handed out (found again by kind, cell and turn once the command applied),
//! and the inverse of a removal is the same placement again. Strokes and
//! placements share one timeline: each placement is stamped with the newest
//! stroke recorded before it, so Undo takes back whichever is newer and Redo
//! lays again whichever is older. Plots export as buildings and props as the
//! world's props: "Export world" needs nothing more from here.
use super::{ShapeTools, picking};
use crate::IslandMode;
use crate::game_ui::icon::{self, Icon};
use crate::game_ui::sound::{self, UiSound};
use crate::game_ui::tokens::{self as t, Status, Surface};
use crate::game_ui::tooltip::Tooltip;
use crate::game_ui::widgets::{self as w, ButtonSpec, Kind as Look, Reach, Tone};
use crate::island_shell::focus::FocusRegion;
use bevy::picking::Pickable;
use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use bevy::ui::InteractionDisabled;
use bevy::ui_widgets::Activate;
use island_content::village::{
    civic,
    frontier::{
        self,
        props::{self as prop_data, PlaceProp, RemoveProp},
        shape::{PlacePlot, RemovePlot},
    },
};
use npc_framework::kernel::commands::{CommandQueue, SkippedCommands, submit_deferred};
use npc_framework::prelude::{SimCommand, SimId};
use npc_framework::view_helpers::{ViewSystems, command_ui::DragBox};

/// Width of the panel.
pub(crate) const PANEL_WIDTH: f32 = 248.0;
/// Placements Undo keeps.
const HISTORY: usize = 64;

/// A farm or garden plot (a catalog building with a field), or a prop.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Kind {
    Plot,
    Prop,
}

/// What is in hand.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Tool {
    Place(Kind, String),
    Remove,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum PlaceAction {
    /// The rail tile: take up the last item (the first plot at first), or
    /// put it down.
    Open,
    Take(Kind, String),
    Remove,
    Rotate,
    Leave,
}

/// One placement as the commands describe it: kind, catalog key, cell (a
/// plot's entrance, a prop's first body cell) and quarter turns.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Thing {
    pub(crate) kind: Kind,
    pub(crate) key: String,
    pub(crate) x: i32,
    pub(crate) z: i32,
    pub(crate) rotation: u8,
}

impl Thing {
    pub(crate) fn place_command(&self) -> SimCommand {
        match self.kind {
            Kind::Plot => SimCommand::content(&self.plot()),
            Kind::Prop => SimCommand::content(&self.prop()),
        }
    }
    fn plot(&self) -> PlacePlot {
        PlacePlot {
            plot: self.key.clone(),
            x: self.x,
            z: self.z,
            rotation: self.rotation,
        }
    }
    fn prop(&self) -> PlaceProp {
        PlaceProp {
            prop: self.key.clone(),
            x: self.x,
            z: self.z,
            rotation: self.rotation,
        }
    }
    /// The simulation's verdict on placing it now.
    pub(crate) fn check_place(&self, world: &World) -> Result<(), String> {
        match self.kind {
            Kind::Plot => self.plot().check(world),
            Kind::Prop => self.prop().check(world),
        }
    }
    /// The placed plot or prop of this kind, key, cell and turn.
    pub(crate) fn find(&self, world: &World) -> Option<SimId> {
        match self.kind {
            Kind::Prop => prop_data::props(world)
                .into_iter()
                .find(|prop| {
                    prop.prop == self.key
                        && (prop.x, prop.z, prop.rotation) == (self.x, self.z, self.rotation % 4)
                })
                .map(|prop| prop.id),
            Kind::Plot => civic::buildings(world)
                .into_iter()
                .find(|building| {
                    building.building == self.key
                        && (building.plot.x, building.plot.z, building.plot.rotation)
                            == (self.x, self.z, self.rotation % 4)
                })
                .map(|building| building.id),
        }
    }
}

pub(crate) fn remove_command(kind: Kind, id: SimId) -> SimCommand {
    match kind {
        Kind::Plot => SimCommand::content(&RemovePlot { plot: id }),
        Kind::Prop => SimCommand::content(&RemoveProp { prop: id }),
    }
}

fn check_remove(world: &World, kind: Kind, id: SimId) -> Result<(), String> {
    match kind {
        Kind::Plot => RemovePlot { plot: id }.check(world),
        Kind::Prop => RemoveProp { prop: id }.check(world),
    }
}

/// The plots of the catalog (buildings with a field) and its props, as
/// `(kind, key, name)`, in the catalog's order.
pub(crate) fn catalog(world: &World) -> Vec<(Kind, String, String)> {
    let mut items: Vec<(Kind, String, String)> = civic::catalog(world)
        .iter()
        .filter(|def| def.field.is_some())
        .map(|def| (Kind::Plot, def.id.clone(), def.display.clone()))
        .collect();
    items.extend(
        prop_data::catalog(world)
            .into_iter()
            .map(|def| (Kind::Prop, def.id, def.display)),
    );
    items
}

/// An item's name as players read it ("Garden", "Barrel").
fn name_of(world: &World, kind: Kind, key: &str) -> String {
    catalog(world)
        .into_iter()
        .find(|(of, id, _)| *of == kind && id == key)
        .map_or_else(|| key.replace('_', " "), |(_, _, name)| name)
}

/// The cell in front of a prop whose body starts at `(x, z)`: what the
/// simulation's placement survey treats as its entrance.
pub(crate) fn prop_front(x: i32, z: i32, rotation: u8) -> (i32, i32) {
    match rotation % 4 {
        0 => (x, z - 1),
        1 => (x - 1, z),
        2 => (x, z + 1),
        _ => (x + 1, z),
    }
}

/// The inclusive body cells of a footprint `width` across and `depth`
/// behind the entrance cell `(x, z)`, turned `rotation` quarters: the
/// simulation's rule for buildings, which plots and props share.
pub(crate) fn body(
    width: u32,
    depth: u32,
    x: i32,
    z: i32,
    rotation: u8,
) -> ((i32, i32), (i32, i32)) {
    let (width, depth) = (width as i32, depth as i32);
    let (left, right) = ((width - 1) / 2, width / 2);
    match rotation % 4 {
        0 => ((x - left, z + 1), (x + right, z + depth)),
        1 => ((x + 1, z - left), (x + depth, z + right)),
        2 => ((x - right, z - depth), (x + left, z - 1)),
        _ => ((x - depth, z - right), (x - 1, z + left)),
    }
}

/// Where a placement would lie: body corners and the cell it faces.
pub(crate) fn footprint(
    world: &World,
    thing: &Thing,
) -> Option<((i32, i32), (i32, i32), (i32, i32))> {
    match thing.kind {
        Kind::Plot => civic::placement_plot(world, &thing.key, thing.x, thing.z, thing.rotation)
            .map(|plot| {
                (
                    (plot.lo_x, plot.lo_z),
                    (plot.hi_x, plot.hi_z),
                    (thing.x, thing.z),
                )
            }),
        Kind::Prop => {
            let def = prop_data::catalog(world)
                .into_iter()
                .find(|def| def.id == thing.key)?;
            let front = prop_front(thing.x, thing.z, thing.rotation);
            let (lo, hi) = body(
                def.footprint.width,
                def.footprint.depth,
                front.0,
                front.1,
                thing.rotation,
            );
            Some((lo, hi, front))
        }
    }
}

/// "Facing north" for 0 to "Facing west" for 3: the side the front faces.
pub(crate) fn facing_words(rotation: u8) -> &'static str {
    match rotation % 4 {
        0 => "Facing north",
        1 => "Facing west",
        2 => "Facing south",
        _ => "Facing east",
    }
}

/// Whether the next Undo (or Redo) belongs to a placement rather than a
/// stroke. `stamp` is the placement's: the newest stroke recorded before it.
/// Undo takes back the newer of the two, Redo lays again the older.
pub(crate) fn placement_first(undo: bool, stamp: Option<u64>, stroke: Option<u64>) -> bool {
    match (stamp, stroke) {
        (None, _) => false,
        (Some(_), None) => true,
        (Some(stamp), Some(stroke)) => {
            if undo {
                stamp >= stroke
            } else {
                stamp < stroke
            }
        }
    }
}

/// One placement or removal in the history.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Entry {
    serial: u64,
    pub(crate) thing: Thing,
    /// The placed thing's identity while it stands, once known.
    pub(crate) id: Option<SimId>,
    /// Whether it stands now: Undo and Redo each turn this over.
    pub(crate) present: bool,
    stamp: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Origin {
    Fresh,
    Undo,
    Redo,
}

/// A recorded command waiting for its tick: the entry as it will be once it
/// applies, and the words that say so.
#[derive(Clone, Debug)]
struct Inflight {
    key: (u64, u32),
    entry: Entry,
    origin: Origin,
    words: String,
}

/// The ghost: body corners, the cell it faces, and whether it is refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Ghost {
    pub(crate) lo: (i32, i32),
    pub(crate) hi: (i32, i32),
    pub(crate) front: (i32, i32),
    pub(crate) refused: bool,
}

#[derive(Resource)]
pub(crate) struct Placing {
    pub(crate) tool: Option<Tool>,
    /// The last item in hand, which the rail tile takes up again.
    last: Option<(Kind, String)>,
    pub(crate) rotation: u8,
    pub(crate) hover: Option<(i32, i32)>,
    /// The simulation's answer for a click here now.
    pub(crate) verdict: Result<(), String>,
    pub(crate) ghost: Option<Ghost>,
    /// What the Remove tool would take: its kind, identity and name.
    pub(crate) hovered: Option<(Kind, SimId, String)>,
    pub(crate) requests: Vec<PlaceAction>,
    inflight: Vec<Inflight>,
    undo: Vec<Entry>,
    redo: Vec<Entry>,
    serial: u64,
}

impl Default for Placing {
    fn default() -> Self {
        Self {
            tool: None,
            last: None,
            rotation: 0,
            hover: None,
            verdict: Ok(()),
            ghost: None,
            hovered: None,
            requests: Vec::new(),
            inflight: Vec::new(),
            undo: Vec::new(),
            redo: Vec::new(),
            serial: 0,
        }
    }
}

impl Placing {
    /// Put the plot or prop in hand down.
    pub(crate) fn put_down(&mut self) {
        self.tool = None;
        self.hover = None;
        self.hovered = None;
        self.ghost = None;
        self.verdict = Ok(());
    }
    /// A stroke was drawn: the placements taken back cannot be laid again.
    pub(crate) fn forget_redo(&mut self) {
        self.redo.clear();
    }
}

/// Whether a plot, a prop or the Remove placed tool is in hand.
pub(crate) fn active(world: &World) -> bool {
    world
        .get_resource::<Placing>()
        .is_some_and(|state| state.tool.is_some())
}

/// Recorded placements still waiting for their tick.
pub(crate) fn busy(world: &World) -> bool {
    world
        .get_resource::<Placing>()
        .is_some_and(|state| !state.inflight.is_empty())
}

/// Whether Undo (or Redo) has a placement to take back (or lay again).
pub(crate) fn can_step(world: &World, undo: bool) -> bool {
    world.get_resource::<Placing>().is_some_and(|state| {
        if undo {
            !state.undo.is_empty()
        } else {
            !state.redo.is_empty()
        }
    })
}

/// Whether the next Undo (or Redo) is a placement's.
pub(crate) fn claims(world: &World, undo: bool) -> bool {
    let Some(state) = world.get_resource::<Placing>() else {
        return false;
    };
    let tools = world.resource::<ShapeTools>();
    let (stamp, stroke) = if undo {
        (
            state.undo.last().map(|entry| entry.stamp),
            tools.history.newest_undo(),
        )
    } else {
        (
            state.redo.last().map(|entry| entry.stamp),
            tools.history.next_redo(),
        )
    };
    placement_first(undo, stamp, stroke)
}

fn words_of(world: &World, thing: &Thing) -> String {
    name_of(world, thing.kind, &thing.key).to_lowercase()
}

fn status(world: &mut World, text: String, refused: bool) {
    world.resource_mut::<ShapeTools>().status = text;
    sound::play(
        world,
        if refused {
            UiSound::Deny
        } else {
            UiSound::Place
        },
    );
}

/// Take back (`undo`) or lay again the newest placement of that side.
pub(crate) fn step(world: &mut World, undo: bool) {
    let popped = {
        let mut state = world.resource_mut::<Placing>();
        if undo {
            state.undo.pop()
        } else {
            state.redo.pop()
        }
    };
    let Some(mut entry) = popped else {
        return;
    };
    let verb = if undo { "Undo" } else { "Redo" };
    let name = words_of(world, &entry.thing);
    if entry.present && entry.id.is_none() {
        entry.id = entry.thing.find(world);
    }
    let (command, check, words) = if entry.present {
        let Some(id) = entry.id else {
            // Gone some other way: nothing to take back.
            status(
                world,
                format!("{verb}: the {name} is no longer there."),
                true,
            );
            return;
        };
        (
            remove_command(entry.thing.kind, id),
            check_remove(world, entry.thing.kind, id),
            format!("Removed the {name}."),
        )
    } else {
        (
            entry.thing.place_command(),
            entry.thing.check_place(world),
            format!("Placed the {name} again."),
        )
    };
    let put_back = |world: &mut World, entry: Entry| {
        let mut state = world.resource_mut::<Placing>();
        if undo {
            state.undo.push(entry);
        } else {
            state.redo.push(entry);
        }
    };
    if let Err(reason) = check {
        put_back(world, entry);
        status(
            world,
            format!(
                "Cannot {}: {}",
                verb.to_lowercase(),
                crate::game_ui::words::sentence(&reason)
            ),
            true,
        );
        return;
    }
    match submit_deferred(world, None, command) {
        Ok(key) => {
            let moved = Entry {
                present: !entry.present,
                id: None,
                ..entry
            };
            let mut state = world.resource_mut::<Placing>();
            if undo {
                state.redo.push(moved.clone());
            } else {
                state.undo.push(moved.clone());
            }
            state.inflight.push(Inflight {
                key,
                entry: moved,
                origin: if undo { Origin::Undo } else { Origin::Redo },
                words,
            });
            world.resource_mut::<ShapeTools>().status = format!("{verb} requested \u{b7} {name}");
        }
        Err(error) => {
            put_back(world, entry);
            status(
                world,
                format!("Cannot {}: {error}", verb.to_lowercase()),
                true,
            );
        }
    }
}

/// What a click at `cell` would do, shown before the click, and the click.
/// `cell` is the ground under the pointer; `None` over the interface.
pub(crate) fn aim(world: &mut World, cell: Option<(i32, i32)>, clicked: bool) {
    let (tool, rotation) = {
        let state = world.resource::<Placing>();
        (state.tool.clone(), state.rotation)
    };
    let mut ghost = None;
    let mut hovered = None;
    // The command a click records, and what applies once it did.
    let mut planned: Option<(SimCommand, Entry, String)> = None;
    let stamp = world.resource::<ShapeTools>().history.issued();
    let serial = world.resource::<Placing>().serial + 1;
    let verdict = match (&tool, cell) {
        (Some(Tool::Place(kind, key)), Some((x, z))) => {
            let thing = Thing {
                kind: *kind,
                key: key.clone(),
                x,
                z,
                rotation,
            };
            let verdict = thing
                .check_place(world)
                .map_err(|reason| crate::game_ui::words::sentence(&reason));
            ghost = footprint(world, &thing).map(|(lo, hi, front)| Ghost {
                lo,
                hi,
                front,
                refused: verdict.is_err(),
            });
            let words = format!("Placed a {}.", words_of(world, &thing));
            planned = Some((
                thing.place_command(),
                Entry {
                    serial,
                    thing,
                    id: None,
                    present: true,
                    stamp,
                },
                words,
            ));
            verdict
        }
        (Some(Tool::Remove), Some(at)) => match placed_at(world, at) {
            None => Err("Nothing placed here: Remove takes plots and props.".to_owned()),
            Some((thing, id, name)) => {
                let verdict = check_remove(world, thing.kind, id)
                    .map_err(|reason| crate::game_ui::words::sentence(&reason));
                hovered = Some((thing.kind, id, name.clone()));
                planned = Some((
                    remove_command(thing.kind, id),
                    Entry {
                        serial,
                        thing,
                        id: None,
                        present: false,
                        stamp,
                    },
                    format!("Removed the {}.", name.to_lowercase()),
                ));
                verdict
            }
        },
        _ => Ok(()),
    };
    {
        let mut state = world.resource_mut::<Placing>();
        state.hover = cell;
        state.ghost = ghost;
        state.hovered = hovered;
        state.verdict = verdict.clone();
    }
    if !clicked || cell.is_none() {
        return;
    }
    let verb = if tool == Some(Tool::Remove) {
        "Not removed"
    } else {
        "Not placed"
    };
    match (verdict, planned) {
        (Ok(()), Some((command, entry, words))) => match submit_deferred(world, None, command) {
            Ok(key) => {
                let mut state = world.resource_mut::<Placing>();
                state.serial = serial;
                state.inflight.push(Inflight {
                    key,
                    entry,
                    origin: Origin::Fresh,
                    words,
                });
            }
            Err(error) => status(world, format!("{verb}: {error}"), true),
        },
        (Err(reason), _) => status(world, format!("{verb}: {reason}"), true),
        _ => {}
    }
}

/// The plot or prop on `cell`: what it is, its identity and its name.
fn placed_at(world: &World, (x, z): (i32, i32)) -> Option<(Thing, SimId, String)> {
    if let Some(prop) = prop_data::props(world).into_iter().find(|prop| {
        (prop.body_min.0..=prop.body_max.0).contains(&x)
            && (prop.body_min.1..=prop.body_max.1).contains(&z)
    }) {
        return Some((
            Thing {
                kind: Kind::Prop,
                key: prop.prop.clone(),
                x: prop.x,
                z: prop.z,
                rotation: prop.rotation,
            },
            prop.id,
            prop.display,
        ));
    }
    civic::buildings(world)
        .into_iter()
        .filter(|building| {
            civic::definition(world, &building.building).is_some_and(|def| def.field.is_some())
        })
        .find(|building| building.plot.contains(x, z))
        .map(|building| {
            let name = name_of(world, Kind::Plot, &building.building);
            (
                Thing {
                    kind: Kind::Plot,
                    key: building.building.clone(),
                    x: building.plot.x,
                    z: building.plot.z,
                    rotation: building.plot.rotation,
                },
                building.id,
                name,
            )
        })
}

/// Every placed plot and prop: body corners and identity, for the marks.
fn placed(world: &World) -> Vec<(SimId, (i32, i32), (i32, i32))> {
    let mut out: Vec<_> = prop_data::props(world)
        .into_iter()
        .map(|prop| (prop.id, prop.body_min, prop.body_max))
        .collect();
    out.extend(
        civic::buildings(world)
            .into_iter()
            .filter(|building| {
                civic::definition(world, &building.building).is_some_and(|def| def.field.is_some())
            })
            .map(|building| {
                (
                    building.id,
                    (building.plot.lo_x, building.plot.lo_z),
                    (building.plot.hi_x, building.plot.hi_z),
                )
            }),
    );
    out
}

/// Match recorded placements with the simulation's outcome: the history
/// learns the identity a placement was given, and a refused Undo or Redo
/// goes back where it was.
pub(crate) fn settle(world: &mut World) {
    if world.resource::<Placing>().inflight.is_empty() {
        return;
    }
    let pending = std::mem::take(&mut world.resource_mut::<Placing>().inflight);
    let first = pending.iter().map(|f| f.key.0).min().unwrap_or(0);
    let skipped: Vec<((u64, u32), String)> = world
        .get_resource::<SkippedCommands>()
        .map(|log| {
            log.entries
                .iter()
                .rev()
                .take_while(|entry| entry.tick >= first)
                .map(|entry| ((entry.tick, entry.seq), entry.error.to_string()))
                .collect()
        })
        .unwrap_or_default();
    let mut waiting = Vec::new();
    for flight in pending {
        let queued = world
            .get_resource::<CommandQueue>()
            .is_some_and(|queue| queue.pending.contains_key(&flight.key));
        if queued {
            waiting.push(flight);
            continue;
        }
        if let Some((_, reason)) = skipped.iter().find(|(skip, _)| *skip == flight.key) {
            if flight.origin != Origin::Fresh {
                // Nothing changed: the entry goes back, as it was.
                let id = if flight.entry.present {
                    None
                } else {
                    flight.entry.thing.find(world)
                };
                let mut state = world.resource_mut::<Placing>();
                let (from, to) = if flight.origin == Origin::Undo {
                    (&mut state.redo, Origin::Undo)
                } else {
                    (&mut state.undo, Origin::Redo)
                };
                if let Some(at) = from
                    .iter()
                    .rposition(|entry| entry.serial == flight.entry.serial)
                {
                    let mut entry = from.remove(at);
                    entry.present = !entry.present;
                    entry.id = id;
                    if to == Origin::Undo {
                        state.undo.push(entry);
                    } else {
                        state.redo.push(entry);
                    }
                }
            }
            status(
                world,
                format!("Not applied: {}", crate::game_ui::words::sentence(reason)),
                true,
            );
            continue;
        }
        let id = if flight.entry.present {
            flight.entry.thing.find(world)
        } else {
            None
        };
        match flight.origin {
            Origin::Fresh => {
                world.resource_mut::<ShapeTools>().history.clear_redo();
                let mut state = world.resource_mut::<Placing>();
                state.redo.clear();
                state.undo.push(Entry { id, ..flight.entry });
                if state.undo.len() > HISTORY {
                    state.undo.remove(0);
                }
            }
            Origin::Undo | Origin::Redo => {
                let mut state = world.resource_mut::<Placing>();
                let stack = if flight.origin == Origin::Undo {
                    &mut state.redo
                } else {
                    &mut state.undo
                };
                if let Some(entry) = stack
                    .iter_mut()
                    .rev()
                    .find(|entry| entry.serial == flight.entry.serial)
                {
                    entry.id = id;
                }
            }
        }
        status(world, flight.words, false);
    }
    world.resource_mut::<Placing>().inflight.extend(waiting);
}

// --- Interface ---------------------------------------------------------------

#[derive(Component, Clone)]
struct PlaceControl(PlaceAction);
#[derive(Component)]
struct PanelRoot;
#[derive(Component)]
struct ItemsGrid(Kind);
#[derive(Component)]
struct FacingText;
#[derive(Component)]
struct HintText;
#[derive(Component)]
struct TagRoot;
#[derive(Component)]
struct TagLine;
#[derive(Component)]
struct TagReason;

/// The catalog the grids were built from.
#[derive(Resource, Default)]
struct Built(Vec<(Kind, String, String)>);

pub(super) fn plugin(app: &mut App) {
    app.init_resource::<Placing>()
        .init_resource::<Built>()
        .init_resource::<Marks>()
        .add_observer(activate)
        .add_systems(Startup, spawn)
        .add_systems(
            Update,
            (keys, pointer)
                .chain()
                .in_set(ViewSystems::OverlayInput)
                .before(crate::island_shell::ShellInput)
                .before(super::keys)
                .before(super::pointer)
                .run_if(not(crate::player_hud::active)),
        )
        .add_systems(
            Update,
            (process, refresh, mark, gizmo)
                .chain()
                .in_set(ViewSystems::Present)
                .after(super::resources::process),
        );
}

fn editing(world: &World) -> bool {
    world.get_resource::<IslandMode>() == Some(&IslandMode::Editor) && frontier::enabled(world)
}

/// The rail's Place tile.
pub(super) fn rail_tile(world: &mut World, rail: Entity) {
    let tile = w::dock_tile(
        world,
        rail,
        "Place",
        Icon::Field,
        Some("L"),
        PlaceControl(PlaceAction::Open),
    );
    world.entity_mut(tile.entity).insert(
        Tooltip::new(
            "Place plots and props",
            "Lay out farm and garden plots and set props: barrels, fences, flower beds. R turns what you hold.",
        )
        .icon(Icon::Field)
        .key("L")
        .detail([
            "Part of Undo and Redo, with the strokes. Exported with the world.",
        ]),
    );
}

fn spawn(world: &mut World) {
    let panel = world
        .spawn((
            Name::new("Shape plots and props"),
            PanelRoot,
            FocusRegion::Palette,
            GlobalZIndex(t::layer::CHROME),
            w::chrome(
                Node {
                    position_type: PositionType::Absolute,
                    top: px(t::TOP_BAR + t::SCREEN_MARGIN),
                    left: px(t::SCREEN_MARGIN + 76.0 + t::SCREEN_MARGIN),
                    width: px(PANEL_WIDTH),
                    padding: UiRect::all(px(10)),
                    row_gap: px(6),
                    flex_direction: FlexDirection::Column,
                    display: Display::None,
                    ..default()
                },
                t::RADIUS_LG,
            ),
        ))
        .id();
    w::label(
        world,
        panel,
        "Plots and props",
        t::SUBHEADING,
        Tone::text(Surface::Chrome),
    );
    for (kind, caption) in [(Kind::Plot, "Plots"), (Kind::Prop, "Props")] {
        w::label(
            world,
            panel,
            caption,
            t::CAPTION,
            Tone::secondary(Surface::Chrome),
        );
        world.spawn((
            ChildOf(panel),
            ItemsGrid(kind),
            Node {
                width: percent(100),
                flex_wrap: FlexWrap::Wrap,
                column_gap: px(4),
                row_gap: px(4),
                ..default()
            },
        ));
    }
    let row = world
        .spawn((
            ChildOf(panel),
            Node {
                width: percent(100),
                align_items: AlignItems::Center,
                column_gap: px(6),
                ..default()
            },
        ))
        .id();
    let turn = w::button(
        world,
        row,
        ButtonSpec::new("Turn", Look::Secondary, Surface::Chrome).reach(Reach::Ring),
        PlaceControl(PlaceAction::Rotate),
    );
    world.entity_mut(turn).insert((
        Name::new("Game: Turn"),
        Tooltip::new(
            "Turn",
            "Turn the plot or prop a quarter. Its front is marked on the ground.",
        )
        .key("R"),
    ));
    let facing = w::label(world, row, "", t::BODY, Tone::text(Surface::Chrome));
    world.entity_mut(facing).insert(FacingText);
    let remove = w::button(
        world,
        panel,
        ButtonSpec::new("Remove placed", Look::Secondary, Surface::Chrome).reach(Reach::Ring),
        PlaceControl(PlaceAction::Remove),
    );
    world.entity_mut(remove).insert((
        Name::new("Game: Remove placed"),
        Tooltip::new(
            "Remove placed",
            "Take away a plot or prop you click. A plot with seed, a crop or a worker stays, and the founding fields stay.",
        )
        .detail(["Undo puts it back."]),
    ));
    let hint = w::paragraph(world, panel, "", t::DENSE, Tone::secondary(Surface::Chrome));
    world.entity_mut(hint).insert(HintText);
    if let Some(mut node) = world.get_mut::<Node>(hint) {
        node.width = px(PANEL_WIDTH - 20.0);
    }
    // The tag beside the cursor: what a click does here, or why not.
    let tag = world
        .spawn((
            Name::new("Placing cursor tag"),
            TagRoot,
            Pickable::IGNORE,
            GlobalZIndex(t::layer::TOOLTIP),
            Outline::new(px(1), px(0), Color::NONE),
            w::chrome(
                Node {
                    position_type: PositionType::Absolute,
                    padding: UiRect::axes(px(10), px(6)),
                    flex_direction: FlexDirection::Column,
                    row_gap: px(3),
                    display: Display::None,
                    ..default()
                },
                t::RADIUS_MD,
            ),
        ))
        .id();
    let line = w::label(world, tag, "", t::BODY, Tone::text(Surface::Chrome));
    world.entity_mut(line).insert((TagLine, Pickable::IGNORE));
    let reason = w::paragraph(
        world,
        tag,
        "",
        t::DENSE,
        Tone::status(Surface::Chrome, Status::Crit),
    );
    world
        .entity_mut(reason)
        .insert((TagReason, Pickable::IGNORE));
    if let Some(mut node) = world.get_mut::<Node>(reason) {
        node.width = px(260);
    }
}

fn activate(
    event: On<Activate>,
    controls: Query<(&PlaceControl, Has<InteractionDisabled>)>,
    mut state: ResMut<Placing>,
) {
    if let Ok((control, false)) = controls.get(event.entity) {
        state.requests.push(control.0.clone());
    }
}

/// L takes up (or puts down) the last plot or prop; with one in hand R turns
/// it and Esc puts it down, before the terrain keys and the shell's Esc.
fn keys(world: &mut World) {
    if !editing(world) {
        return;
    }
    if world
        .get_resource::<bevy::input_focus::InputFocus>()
        .and_then(|focus| focus.get())
        .is_some_and(|entity| world.get::<bevy::text::EditableText>(entity).is_some())
    {
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
    ]) {
        return;
    }
    let holding = active(world);
    let pressed: Vec<(KeyCode, PlaceAction)> = [
        (KeyCode::KeyL, PlaceAction::Open),
        (KeyCode::KeyR, PlaceAction::Rotate),
        (KeyCode::Escape, PlaceAction::Leave),
    ]
    .into_iter()
    .filter(|(key, action)| keys.just_pressed(*key) && (*action == PlaceAction::Open || holding))
    .collect();
    if pressed.is_empty() {
        return;
    }
    let mut keys = world.resource_mut::<ButtonInput<KeyCode>>();
    for (key, _) in &pressed {
        keys.reset(*key);
    }
    world
        .resource_mut::<Placing>()
        .requests
        .extend(pressed.into_iter().map(|(_, action)| action));
}

fn pointer(world: &mut World) {
    if !editing(world) || !active(world) {
        let mut state = world.resource_mut::<Placing>();
        state.hover = None;
        state.hovered = None;
        state.ghost = None;
        return;
    }
    let ui_hovered = world
        .query_filtered::<&Hovered, With<Node>>()
        .iter(world)
        .any(Hovered::get);
    let cell = if ui_hovered {
        None
    } else {
        picking::cursor_cell(world).filter(|(x, z)| picking::terrain_cell(world, *x, *z).is_some())
    };
    let clicked = world
        .get_resource::<ButtonInput<MouseButton>>()
        .is_some_and(|mouse| mouse.just_pressed(MouseButton::Left));
    aim(world, cell, clicked);
    if clicked {
        // A click of the tool never also selects or orders anybody.
        if let Some(mut mouse) = world.get_resource_mut::<ButtonInput<MouseButton>>() {
            mouse.clear_just_pressed(MouseButton::Left);
            mouse.clear_just_released(MouseButton::Left);
        }
        if let Some(mut drag) = world.get_resource_mut::<DragBox>() {
            drag.start = None;
        }
    }
}

pub(crate) fn process(world: &mut World) {
    let requests = std::mem::take(&mut world.resource_mut::<Placing>().requests);
    for request in requests {
        let taken = match request {
            PlaceAction::Open => {
                if active(world) {
                    world.resource_mut::<Placing>().put_down();
                    false
                } else {
                    let first = catalog(world)
                        .into_iter()
                        .next()
                        .map(|(kind, key, _)| (kind, key));
                    let item = world.resource::<Placing>().last.clone().or(first);
                    match item {
                        Some((kind, key)) => {
                            world.resource_mut::<Placing>().tool = Some(Tool::Place(kind, key));
                            true
                        }
                        None => false,
                    }
                }
            }
            PlaceAction::Take(kind, key) => {
                let mut state = world.resource_mut::<Placing>();
                state.last = Some((kind, key.clone()));
                state.tool = Some(Tool::Place(kind, key));
                true
            }
            PlaceAction::Remove => {
                let mut state = world.resource_mut::<Placing>();
                state.tool = if state.tool == Some(Tool::Remove) {
                    None
                } else {
                    Some(Tool::Remove)
                };
                state.tool.is_some()
            }
            PlaceAction::Rotate => {
                let mut state = world.resource_mut::<Placing>();
                state.rotation = (state.rotation + 1) % 4;
                false
            }
            PlaceAction::Leave => {
                world.resource_mut::<Placing>().put_down();
                false
            }
        };
        if taken {
            // One tool in hand: the resource tools rest.
            if let Some(mut resources) = world.get_resource_mut::<super::resources::Resources>() {
                resources.mode = None;
            }
        }
    }
    settle(world);
}

fn refresh(world: &mut World) {
    let shown = editing(world) && !crate::player_hud::active(world);
    let state = world.resource::<Placing>();
    let (tool, rotation, verdict, hover, hovered) = (
        state.tool.clone(),
        state.rotation,
        state.verdict.clone(),
        state.hover,
        state.hovered.clone(),
    );
    let open = shown && tool.is_some();
    let top = crate::island_shell::content_top(world);
    for mut node in world
        .query_filtered::<&mut Node, With<PanelRoot>>()
        .iter_mut(world)
    {
        let display = if open { Display::Flex } else { Display::None };
        if node.display != display {
            node.display = display;
        }
        if node.top != px(top) {
            node.top = px(top);
        }
    }
    let controls: Vec<(Entity, PlaceAction)> = world
        .query::<(Entity, &PlaceControl)>()
        .iter(world)
        .map(|(entity, control)| (entity, control.0.clone()))
        .collect();
    for (entity, action) in controls {
        let (on, enabled) = match &action {
            PlaceAction::Open => (tool.is_some(), true),
            PlaceAction::Take(kind, key) => (
                tool.as_ref() == Some(&Tool::Place(*kind, key.clone())),
                true,
            ),
            PlaceAction::Remove => (tool == Some(Tool::Remove), true),
            PlaceAction::Rotate => (false, matches!(tool, Some(Tool::Place(..)))),
            PlaceAction::Leave => (false, true),
        };
        w::set_state(world, entity, on, false);
        w::set_enabled(world, entity, enabled);
    }
    if !shown {
        hide_tag(world);
        return;
    }
    // The grids, built once the world's catalog is known.
    let items = catalog(world);
    if world.resource::<Built>().0 != items {
        let grids: Vec<(Entity, Kind)> = world
            .query::<(Entity, &ItemsGrid)>()
            .iter(world)
            .map(|(entity, grid)| (entity, grid.0))
            .collect();
        for (grid, of) in grids {
            world.entity_mut(grid).despawn_children();
            for (kind, key, name) in items.iter().filter(|(kind, ..)| *kind == of) {
                let tile = w::control(
                    world,
                    grid,
                    name,
                    Look::DockTile,
                    Surface::Chrome,
                    Reach::Ring,
                    Node {
                        width: px(70),
                        height: px(50),
                        flex_direction: FlexDirection::Column,
                        align_items: AlignItems::Center,
                        justify_content: JustifyContent::Center,
                        row_gap: px(2),
                        border: UiRect::all(px(1)),
                        border_radius: BorderRadius::all(px(t::RADIUS_MD)),
                        ..default()
                    },
                    PlaceControl(PlaceAction::Take(*kind, key.clone())),
                );
                // A generic picture for every entry: data adds entries.
                let picture = if *kind == Kind::Plot {
                    Icon::Field
                } else {
                    Icon::Storage
                };
                icon::icon(world, tile, picture, 20.0, Tone::text(Surface::Chrome));
                let caption = w::label(
                    world,
                    tile,
                    name,
                    t::CAPTION,
                    Tone::secondary(Surface::Chrome),
                );
                world.entity_mut(caption).insert(w::Part::Label);
                let sentence = if *kind == Kind::Plot {
                    format!(
                        "Lay out a {} here: finished at once and empty, for the village to sow.",
                        name.to_lowercase()
                    )
                } else {
                    format!("Set a {} here.", name.to_lowercase())
                };
                world.entity_mut(tile).insert(
                    Tooltip::new(name.clone(), sentence)
                        .detail(["R turns it; Undo takes it back."]),
                );
            }
        }
        world.resource_mut::<Built>().0 = items;
    }
    let facing: Vec<Entity> = world
        .query_filtered::<Entity, With<FacingText>>()
        .iter(world)
        .collect();
    for entity in facing {
        w::set_text(world, entity, facing_words(rotation));
    }
    let hint = match tool {
        Some(Tool::Remove) => "Click a marked plot or prop to take it away. Undo puts it back.",
        _ => {
            "Click the ground to place: green where it can go, red with the reason where not. R turns it; Undo takes it back."
        }
    };
    let hints: Vec<Entity> = world
        .query_filtered::<Entity, With<HintText>>()
        .iter(world)
        .collect();
    for entity in hints {
        w::set_text(world, entity, hint);
    }
    // The cursor tag.
    let line = match (&tool, hover) {
        (Some(Tool::Place(kind, key)), Some(_)) => Some(format!(
            "Place {} \u{b7} {}",
            name_of(world, *kind, key).to_lowercase(),
            facing_words(rotation).to_lowercase()
        )),
        (Some(Tool::Remove), Some(_)) => Some(hovered.as_ref().map_or_else(
            || "Remove".to_owned(),
            |(_, _, name)| format!("Remove the {}", name.to_lowercase()),
        )),
        _ => None,
    };
    let cursor = world
        .query::<&Window>()
        .iter(world)
        .find_map(Window::cursor_position);
    let scale = crate::game_ui::scale::layout(world).scale.max(0.1);
    let reason = verdict.err();
    let lines: Vec<Entity> = world
        .query_filtered::<Entity, With<TagLine>>()
        .iter(world)
        .collect();
    for entity in lines {
        w::set_text(world, entity, line.as_deref().unwrap_or_default());
    }
    let reasons: Vec<Entity> = world
        .query_filtered::<Entity, With<TagReason>>()
        .iter(world)
        .collect();
    for entity in reasons {
        w::set_text(world, entity, reason.as_deref().unwrap_or_default());
        w::show(world, entity, reason.is_some());
    }
    let tags: Vec<Entity> = world
        .query_filtered::<Entity, With<TagRoot>>()
        .iter(world)
        .collect();
    for tag in tags {
        let edge = if reason.is_some() {
            t::CRIT_LIGHT
        } else {
            Color::NONE
        };
        if let Some(mut outline) = world.get_mut::<Outline>(tag)
            && outline.color != edge
        {
            outline.color = edge;
        }
        if let Some(mut node) = world.get_mut::<Node>(tag) {
            match (cursor, line.is_some()) {
                (Some(cursor), true) => {
                    node.display = Display::Flex;
                    node.left = px(cursor.x / scale + 18.0);
                    node.top = px(cursor.y / scale + 18.0);
                }
                _ => node.display = Display::None,
            }
        }
    }
}

fn hide_tag(world: &mut World) {
    for mut node in world
        .query_filtered::<&mut Node, With<TagRoot>>()
        .iter_mut(world)
    {
        if node.display != Display::None {
            node.display = Display::None;
        }
    }
}

/// What the gizmos draw this frame: the ghost and the placed plots and props
/// (and whether one is under the Remove tool), as cell rectangles at height.
#[derive(Resource, Default)]
struct Marks {
    ghost: Option<(Ghost, f32)>,
    placed: Vec<((i32, i32), (i32, i32), f32, bool)>,
}

fn height_at(world: &World, (x, z): (i32, i32)) -> f32 {
    picking::terrain_cell(world, x, z).map_or(0.0, |cell| cell.height_mm.max(0) as f32 / 1000.0)
        + 0.08
}

fn mark(world: &mut World) {
    let state = world.resource::<Placing>();
    let open = editing(world) && state.tool.is_some() && !crate::player_hud::active(world);
    let (ghost, hovered) = (state.ghost, state.hovered.as_ref().map(|(_, id, _)| *id));
    let marks = if open {
        Marks {
            ghost: ghost.map(|ghost| (ghost, height_at(world, ghost.front))),
            placed: placed(world)
                .into_iter()
                .map(|(id, lo, hi)| (lo, hi, height_at(world, lo), hovered == Some(id)))
                .collect(),
        }
    } else {
        Marks::default()
    };
    *world.resource_mut::<Marks>() = marks;
}

fn rect(gizmos: &mut Gizmos, lo: (i32, i32), hi: (i32, i32), y: f32, inset: f32, color: Color) {
    let (x0, z0) = (lo.0 as f32 + inset, lo.1 as f32 + inset);
    let (x1, z1) = ((hi.0 + 1) as f32 - inset, (hi.1 + 1) as f32 - inset);
    let corners = [
        Vec3::new(x0, y, z0),
        Vec3::new(x1, y, z0),
        Vec3::new(x1, y, z1),
        Vec3::new(x0, y, z1),
    ];
    for at in 0..4 {
        gizmos.line(corners[at], corners[(at + 1) % 4], color);
    }
}

fn gizmo(marks: Res<Marks>, mut gizmos: Gizmos) {
    for (lo, hi, y, picked) in &marks.placed {
        let color = if *picked { t::CRIT_LIGHT } else { t::HONEY_500 };
        rect(&mut gizmos, *lo, *hi, *y, 0.05, color);
        if *picked {
            // The one the click takes: a second edge, not colour alone.
            rect(&mut gizmos, *lo, *hi, *y, 0.2, color);
        }
    }
    if let Some((ghost, y)) = marks.ghost {
        let color = if ghost.refused {
            t::CRIT_LIGHT
        } else {
            t::GOOD_LIGHT
        };
        rect(&mut gizmos, ghost.lo, ghost.hi, y, 0.02, color);
        if ghost.refused {
            // Refused: crossed, so the refusal is not colour alone.
            let (a, b) = (
                Vec3::new(ghost.lo.0 as f32, y, ghost.lo.1 as f32),
                Vec3::new((ghost.hi.0 + 1) as f32, y, (ghost.hi.1 + 1) as f32),
            );
            gizmos.line(a, b, color);
            gizmos.line(Vec3::new(a.x, y, b.z), Vec3::new(b.x, y, a.z), color);
        } else {
            rect(&mut gizmos, ghost.lo, ghost.hi, y, 0.18, color);
        }
        // The front: a ring on the cell it faces.
        let front = Vec3::new(ghost.front.0 as f32 + 0.5, y, ghost.front.1 as f32 + 0.5);
        gizmos.circle(
            Isometry3d::new(front, Quat::from_rotation_arc(Vec3::Z, Vec3::Y)),
            0.3,
            color,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use npc_framework::kernel::hash::state_hash;
    use npc_framework::prelude::{SimPos, run_ticks};

    fn living() -> App {
        let mut app = island_content::new_sim_app(7, "living_world");
        app.init_resource::<ShapeTools>()
            .init_resource::<Placing>()
            .insert_resource(IslandMode::Editor);
        app
    }

    /// Apply what was recorded, then settle.
    fn apply(app: &mut App) {
        for _ in 0..20 {
            settle(app.world_mut());
            if !busy(app.world()) {
                return;
            }
            run_ticks(app.world_mut(), 1);
        }
        panic!("placements never settled");
    }

    /// A cell near a settlement where `thing` may be placed.
    fn placeable(world: &World, kind: Kind, key: &str) -> Thing {
        let centres: Vec<SimPos> = world
            .try_query::<(&frontier::Settlement, &SimPos)>()
            .unwrap()
            .iter(world)
            .map(|(_, pos)| *pos)
            .collect();
        for centre in centres {
            let (cx, cz) = (centre.x.div_euclid(1000), centre.z.div_euclid(1000));
            for radius in 6..30 {
                for dz in -radius..=radius {
                    for dx in [-radius, radius] {
                        let thing = Thing {
                            kind,
                            key: key.to_owned(),
                            x: cx + dx,
                            z: cz + dz,
                            rotation: 0,
                        };
                        if thing.check_place(world).is_ok() {
                            return thing;
                        }
                    }
                }
            }
        }
        panic!("nowhere to place {key}");
    }

    #[test]
    fn the_palette_lists_the_catalogs_plots_and_props() {
        let app = living();
        let items = catalog(app.world());
        let plots: Vec<&str> = items
            .iter()
            .filter(|(kind, ..)| *kind == Kind::Plot)
            .map(|(_, key, _)| key.as_str())
            .collect();
        assert!(
            plots.contains(&"field") && plots.contains(&"garden"),
            "{plots:?}"
        );
        let props = items
            .iter()
            .filter(|(kind, ..)| *kind == Kind::Prop)
            .count();
        assert_eq!(props, prop_data::catalog(app.world()).len());
        assert!(props >= 7, "the shipped props");
        assert!(items.iter().all(|(_, _, name)| !name.is_empty()));
    }

    #[test]
    fn the_ghost_lies_where_the_simulation_puts_the_prop() {
        let mut app = living();
        let barrel = placeable(app.world(), Kind::Prop, "barrel");
        let (lo, hi, _) = footprint(app.world(), &barrel).expect("a barrel's ghost");
        app.world_mut().resource_mut::<Placing>().tool =
            Some(Tool::Place(Kind::Prop, "barrel".into()));
        aim(app.world_mut(), Some((barrel.x, barrel.z)), true);
        apply(&mut app);
        let placed = prop_data::props(app.world());
        assert_eq!(placed.len(), 1);
        assert_eq!((placed[0].body_min, placed[0].body_max), (lo, hi));
        // Every turn of a wider footprint covers its body from the front.
        for rotation in 0..4 {
            let (lo, hi) = body(3, 2, 0, 0, rotation);
            assert_eq!((hi.0 - lo.0 + 1) * (hi.1 - lo.1 + 1), 6, "{rotation}");
            assert!(!(lo.0..=hi.0).contains(&0) || !(lo.1..=hi.1).contains(&0));
        }
        assert_eq!(facing_words(0), "Facing north");
        assert_eq!(facing_words(5), "Facing west");
    }

    #[test]
    fn a_placed_prop_is_undone_by_its_removal_and_redone_by_placing_it_again() {
        let mut app = living();
        let barrel = placeable(app.world(), Kind::Prop, "barrel");
        app.world_mut().resource_mut::<Placing>().tool =
            Some(Tool::Place(Kind::Prop, "barrel".into()));
        let before = state_hash(app.world_mut());
        aim(app.world_mut(), Some((barrel.x, barrel.z)), false);
        assert_eq!(
            before,
            state_hash(app.world_mut()),
            "aiming asks the simulation and changes nothing"
        );
        assert!(app.world().resource::<Placing>().verdict.is_ok());
        assert!(
            app.world()
                .resource::<Placing>()
                .ghost
                .is_some_and(|ghost| !ghost.refused)
        );
        aim(app.world_mut(), Some((barrel.x, barrel.z)), true);
        let queued: Vec<serde_json::Value> = app
            .world()
            .resource::<CommandQueue>()
            .pending
            .values()
            .map(|command| serde_json::to_value(command).unwrap())
            .collect();
        assert_eq!(
            queued,
            [serde_json::json!({
                "type": "PlaceProp", "prop": "barrel", "x": barrel.x, "z": barrel.z, "rotation": 0
            })]
        );
        apply(&mut app);
        let first = barrel.find(app.world()).expect("placed");
        assert!(claims(app.world(), true), "Undo takes the placement back");
        assert!(
            app.world()
                .resource::<ShapeTools>()
                .status
                .starts_with("Placed a barrel")
        );

        step(app.world_mut(), true);
        let queued: Vec<serde_json::Value> = app
            .world()
            .resource::<CommandQueue>()
            .pending
            .values()
            .map(|command| serde_json::to_value(command).unwrap())
            .collect();
        assert_eq!(
            queued,
            [serde_json::json!({"type": "RemoveProp", "prop": first.0})]
        );
        apply(&mut app);
        assert_eq!(barrel.find(app.world()), None, "undone");
        assert!(claims(app.world(), false));

        step(app.world_mut(), false);
        apply(&mut app);
        let again = barrel.find(app.world()).expect("redone");
        assert_ne!(again, first, "a new identity");
        // And undone again: the history learnt the new identity.
        step(app.world_mut(), true);
        apply(&mut app);
        assert_eq!(barrel.find(app.world()), None);
    }

    #[test]
    fn a_garden_plot_is_laid_out_and_removed_with_the_remove_tool_and_put_back_by_undo() {
        let mut app = living();
        let garden = placeable(app.world(), Kind::Plot, "garden");
        app.world_mut().resource_mut::<Placing>().tool =
            Some(Tool::Place(Kind::Plot, "garden".into()));
        aim(app.world_mut(), Some((garden.x, garden.z)), true);
        apply(&mut app);
        let id = garden.find(app.world()).expect("laid out");
        let (lo, _, _) = footprint(app.world(), &garden).unwrap();

        app.world_mut().resource_mut::<Placing>().tool = Some(Tool::Remove);
        aim(app.world_mut(), Some(lo), false);
        assert_eq!(
            app.world()
                .resource::<Placing>()
                .hovered
                .as_ref()
                .map(|(kind, of, _)| (*kind, *of)),
            Some((Kind::Plot, id))
        );
        aim(app.world_mut(), Some(lo), true);
        apply(&mut app);
        assert_eq!(garden.find(app.world()), None, "removed");
        // Undo asks the simulation to lay the same garden out again. Today it
        // refuses ("The footprint covers an entrance or a public place": the
        // removal leaves something behind; reported to the simulation). Either
        // way the history stays true: laid out again, or still undoable with
        // the reason said.
        step(app.world_mut(), true);
        apply(&mut app);
        let status = app.world().resource::<ShapeTools>().status.clone();
        if garden.find(app.world()).is_none() {
            assert!(status.starts_with("Cannot undo: "), "{status}");
            assert!(
                can_step(app.world(), true),
                "the removal can still be undone"
            );
            assert!(app.world().resource::<CommandQueue>().pending.is_empty());
        }
    }

    #[test]
    fn a_refused_spot_says_why_and_records_nothing() {
        let mut app = living();
        app.world_mut().resource_mut::<Placing>().tool =
            Some(Tool::Place(Kind::Prop, "barrel".into()));
        // Far out at sea.
        aim(app.world_mut(), Some((5_000, 5_000)), true);
        let state = app.world().resource::<Placing>();
        let reason = state.verdict.clone().expect_err("refused");
        assert!(reason.ends_with('.') && reason.chars().next().unwrap().is_uppercase());
        assert!(app.world().resource::<CommandQueue>().pending.is_empty());
        assert!(
            app.world()
                .resource::<ShapeTools>()
                .status
                .starts_with("Not placed: ")
        );
        // Nothing placed under the Remove tool: it says so.
        app.world_mut().resource_mut::<Placing>().tool = Some(Tool::Remove);
        aim(app.world_mut(), Some((0, 0)), false);
        assert!(app.world().resource::<Placing>().verdict.is_err());
    }

    #[test]
    fn strokes_and_placements_share_one_timeline() {
        // Stroke 1, then a placement (stamped 1), then stroke 2.
        // Undo: stroke 2 first, then the placement, then stroke 1.
        assert!(!placement_first(true, Some(1), Some(2)));
        assert!(placement_first(true, Some(1), Some(1)));
        assert!(placement_first(true, Some(0), None));
        // Redo lays the oldest first: stroke 1, the placement, stroke 2.
        assert!(!placement_first(false, Some(1), Some(1)));
        assert!(placement_first(false, Some(1), Some(2)));
        assert!(!placement_first(false, None, Some(2)));
    }
}
