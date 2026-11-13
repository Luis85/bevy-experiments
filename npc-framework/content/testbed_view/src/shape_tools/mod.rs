//! Shape mode: a tool palette, drag painting and undo over the island terrain.
//!
//! The view plans every stroke from read-only terrain queries and changes the
//! world only by recording absolute terrain commands: one `Terraform` disc
//! when a dab ends at one value, otherwise one atomic `TerraformCells` batch.
//! Undo and redo are one further batch each. Relative tools, falloff and undo
//! are therefore replayable inputs like any other. Strokes are rate-limited
//! and bounded; queued commands apply when the world runs, and the
//! simulation's own refusal reason is shown beside the cursor.
pub(crate) mod model;
pub(crate) mod palette;
pub(crate) mod picking;
pub(crate) mod placing;
pub(crate) mod resources;
#[cfg(test)]
mod tests;

use crate::IslandMode;
use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use model::{Brush, Cell, Command, Falloff, History, Limits, Step, Stroke, Tool};
use npc_framework::kernel::commands::{CommandQueue, SkippedCommands, submit_deferred};
use npc_framework::prelude::SimCommand;
use npc_framework::view_helpers::{ViewSystems, command_ui::DragBox};
use std::collections::{BTreeMap, VecDeque};
use std::time::Instant;
use testbed_content::village::{
    colony,
    frontier::terrain::{TerraformCell, TerraformCells},
    terrain::{Terraform, TerrainSurface},
};

/// Seconds between two dabs of one drag.
const DAB_SECS: f32 = 0.10;
/// Seconds before a held, unmoved accumulating tool dabs again.
const REPEAT_SECS: f32 = 0.30;
/// Recorded commands one press-drag-release may issue.
pub(crate) const MAX_STROKE_COMMANDS: usize = 600;
/// Planned commands waiting to be recorded.
const OUTBOX_LIMIT: usize = 400;
/// Recorded commands waiting for a simulation tick. Each one is validated
/// in full by the simulation, so only a few may land in the same tick. A
/// batched dab is one command, so this only ever holds back a profile that
/// still records cell by cell.
const MAX_INFLIGHT: usize = 6;
/// Commands recorded per frame, so a large dab never stalls one frame.
const SUBMIT_PER_FRAME: usize = 6;
/// Strokes kept for undo.
const HISTORY_LIMIT: usize = 32;
/// Seconds a refusal stays beside the cursor.
const REJECTION_SECS: f32 = 5.0;

pub(crate) const SURFACES: [TerrainSurface; 4] = [
    TerrainSurface::Grass,
    TerrainSurface::Soil,
    TerrainSurface::Stone,
    TerrainSurface::Water,
];

pub(crate) fn surface_name(surface: TerrainSurface) -> &'static str {
    match surface {
        TerrainSurface::Grass => "Grass",
        TerrainSurface::Soil => "Soil",
        TerrainSurface::Stone => "Stone",
        TerrainSurface::Water => "Water",
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Action {
    Tool(Tool),
    Surface(TerrainSurface),
    NextSurface,
    Radius(i32),
    /// The size slider: the radius itself.
    SetRadius(u32),
    Falloff,
    /// The Edge switch: one of the two, where `Falloff` (key H) flips.
    Edge(Falloff),
    /// The Strength switch: the height of one full Raise / Lower dab, mm.
    Strength(i32),
    Undo,
    Redo,
}

struct ActiveStroke {
    cells: Stroke,
    anchor_mm: i32,
    last_cell: Option<(i32, i32)>,
    since_dab: f32,
    commands: usize,
}

/// Where a queued command came from, so its outcome reaches the right place.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Origin {
    Dab,
    /// An undo or redo of `stroke`. `whole` when the step is this one
    /// command, so a refusal means the world did not change at all.
    History {
        stroke: u64,
        step: Step,
        whole: bool,
    },
}

/// A planned command and the edit (a dab or a history step) it belongs to.
#[derive(Clone, Debug, PartialEq)]
struct Queued {
    command: Command,
    origin: Origin,
    edit: u64,
    /// For a dab: what this command changes, cell by cell, so a refusal can
    /// take it back out of its stroke.
    changes: Vec<model::Edit>,
}

/// One edit on its way from the pointer to the terrain mesh.
struct Probe {
    began: Instant,
    /// Commands of the edit the simulation has not answered yet.
    left: usize,
    /// Terrain revision that holds the whole edit, once it is known.
    revision: Option<u64>,
}

/// Review aid (`ISLAND_VIEW_STATS=1`): how long edits take from the frame
/// that planned them to the frame whose terrain mesh shows them. Host time
/// only; nothing here reaches the simulation.
#[derive(Default)]
struct Latency {
    enabled: bool,
    next: u64,
    waiting: BTreeMap<u64, Probe>,
    /// Milliseconds, one per edit that reached the mesh.
    samples: Vec<f32>,
    commands: usize,
    cells: usize,
    refused: usize,
    /// Dabs the view dropped because its own queue or stroke bound was full.
    dropped: usize,
    label: String,
}

/// Median, 95th percentile and maximum of latency samples, in milliseconds.
pub(crate) fn latency_summary(samples: &[f32]) -> Option<(f32, f32, f32)> {
    let mut sorted = samples.to_vec();
    sorted.sort_by(f32::total_cmp);
    let last = sorted.len().checked_sub(1)?;
    let at = |fraction: f32| sorted[((last as f32) * fraction).round() as usize];
    Some((at(0.5), at(0.95), sorted[last]))
}

#[derive(Clone, Debug, PartialEq)]
struct PreviewKey {
    cell: (i32, i32),
    brush: Brush,
    revision: u64,
    planned: usize,
}

#[derive(Resource)]
pub(crate) struct ShapeTools {
    pub(crate) brush: Brush,
    pub(crate) hover: Option<(i32, i32)>,
    pub(crate) preview_error: Option<String>,
    preview_key: Option<PreviewKey>,
    /// World-space centres of the cells under the brush, for the outline.
    outline: Vec<Vec3>,
    stroke: Option<ActiveStroke>,
    history: History,
    outbox: VecDeque<Queued>,
    inflight: VecDeque<((u64, u32), Queued)>,
    /// Review aid (`ISLAND_VIEW_SHAPE_BATCH=off`): record uneven edits cell
    /// by cell even where the batch command exists, to compare the two.
    per_cell: bool,
    latency: Latency,
    /// Values already planned but not yet applied, so relative tools keep
    /// building on their own queued result instead of a stale height.
    expected: BTreeMap<(i32, i32), Cell>,
    rejection: Option<(String, f32)>,
    pub(crate) status: String,
    pub(crate) requests: Vec<Action>,
    epoch: Option<u64>,
    applied: u64,
    refused: u64,
}

impl Default for ShapeTools {
    fn default() -> Self {
        Self {
            brush: Brush::default(),
            hover: None,
            preview_error: None,
            preview_key: None,
            outline: Vec::new(),
            stroke: None,
            history: History::new(HISTORY_LIMIT),
            outbox: VecDeque::new(),
            inflight: VecDeque::new(),
            per_cell: false,
            latency: Latency::default(),
            expected: BTreeMap::new(),
            rejection: None,
            status: String::new(),
            requests: Vec::new(),
            epoch: None,
            applied: 0,
            refused: 0,
        }
    }
}

impl ShapeTools {
    pub(crate) fn can_undo(&self) -> bool {
        self.history.can_undo()
    }
    pub(crate) fn can_redo(&self) -> bool {
        self.history.can_redo()
    }
    pub(crate) fn queued(&self) -> usize {
        self.outbox.len() + self.inflight.len()
    }
    /// Strokes that can be taken back, and how many the history keeps.
    pub(crate) fn history_depth(&self) -> (usize, usize) {
        self.history.depth()
    }
    /// The refusal to show beside the cursor: the live preview first, then
    /// the latest refusal the simulation reported for a recorded edit.
    pub(crate) fn cursor_reason(&self) -> Option<&str> {
        self.preview_error
            .as_deref()
            .or(self.rejection.as_ref().map(|(reason, _)| reason.as_str()))
    }
}

pub(crate) fn plugin(app: &mut App) {
    if !colony::enabled(app.world()) {
        return;
    }
    // Review aids, read once. Neither reaches the simulation.
    let mut tools = ShapeTools {
        per_cell: std::env::var("ISLAND_VIEW_SHAPE_BATCH").as_deref() == Ok("off"),
        ..default()
    };
    tools.latency.enabled = std::env::var("ISLAND_VIEW_STATS").is_ok_and(|value| value != "0");
    app.insert_resource(tools)
        .init_gizmo_group::<ShapeGizmos>()
        .add_systems(Startup, outline_style)
        .init_resource::<IslandMode>()
        .add_systems(
            Update,
            (keys, pointer)
                .chain()
                .in_set(ViewSystems::OverlayInput)
                .after(crate::colony_panel::pointer)
                .run_if(not(crate::player_hud::active))
                .run_if(not(crate::modal_open)),
        )
        .add_systems(
            Update,
            (process, gizmo)
                .chain()
                .in_set(ViewSystems::Present)
                .before(crate::colony_panel::refresh),
        );
    palette::plugin(app);
    resources::plugin(app);
    placing::plugin(app);
}

fn editing(world: &World) -> bool {
    world.get_resource::<IslandMode>() == Some(&IslandMode::Editor)
}

fn sample(world: &World, expected: &BTreeMap<(i32, i32), Cell>, x: i32, z: i32) -> Option<Cell> {
    // Only cells that exist in the terrain can be expected to change.
    let actual = picking::terrain_cell(world, x, z)?;
    Some(expected.get(&(x, z)).copied().unwrap_or(actual))
}

/// The world's command ranges, and whether uneven edits travel as one batch.
fn limits(world: &World) -> Limits {
    let limits = picking::limits(world);
    if world.resource::<ShapeTools>().per_cell {
        limits.per_cell()
    } else {
        limits
    }
}

/// The recorded simulation command for a planned one.
fn sim_command(command: &Command) -> SimCommand {
    match command {
        Command::Disc {
            x,
            z,
            radius,
            height_mm,
            surface,
        } => SimCommand::content(&Terraform {
            x: *x,
            z: *z,
            radius: *radius,
            height_mm: *height_mm,
            surface: *surface,
        }),
        Command::Cells(cells) => SimCommand::content(&batch_command(cells)),
    }
}

fn batch_command(cells: &[((i32, i32), Cell)]) -> TerraformCells {
    TerraformCells {
        cells: cells
            .iter()
            .map(|((x, z), cell)| TerraformCell {
                x: *x,
                z: *z,
                height_mm: cell.height_mm,
                surface: cell.surface,
            })
            .collect(),
    }
}

/// The simulation's own answer to a planned command, without recording it.
fn check(world: &World, command: &Command) -> Result<(), String> {
    match command {
        Command::Disc {
            x,
            z,
            radius,
            height_mm,
            surface,
        } => Terraform {
            x: *x,
            z: *z,
            radius: *radius,
            height_mm: *height_mm,
            surface: *surface,
        }
        .check(world),
        Command::Cells(cells) => batch_command(cells).check(world),
    }
}

/// Queue the commands of one edit and start timing it.
fn enqueue(state: &mut ShapeTools, commands: Vec<Command>, origin: Origin, edits: &[model::Edit]) {
    if commands.is_empty() {
        return;
    }
    let edit = state.latency.next;
    state.latency.next += 1;
    if state.latency.enabled {
        state.latency.commands += commands.len();
        state.latency.cells += commands
            .iter()
            .map(|command| command.footprint().len())
            .sum::<usize>();
        state.latency.waiting.insert(
            edit,
            Probe {
                began: Instant::now(),
                left: commands.len(),
                revision: None,
            },
        );
    }
    state.outbox.extend(commands.into_iter().map(|command| {
        let footprint = command.footprint();
        Queued {
            changes: edits
                .iter()
                .filter(|edit| footprint.contains(&(edit.x, edit.z)))
                .copied()
                .collect(),
            command,
            origin,
            edit,
        }
    }));
}

/// `[` / `]` size the brush, Ctrl+Z / Ctrl+Y walk the history, and the
/// labelled letters choose a tool. Consumed so no other panel reacts to them.
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
    let control = keys.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight]);
    let shift = keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
    let other = keys.any_pressed([
        KeyCode::AltLeft,
        KeyCode::AltRight,
        KeyCode::SuperLeft,
        KeyCode::SuperRight,
    ]);
    if other {
        return;
    }
    let bindings: &[(KeyCode, Action)] = if control {
        &[
            (
                KeyCode::KeyZ,
                if shift { Action::Redo } else { Action::Undo },
            ),
            (KeyCode::KeyY, Action::Redo),
        ]
    } else {
        &[
            (KeyCode::BracketLeft, Action::Radius(-1)),
            (KeyCode::BracketRight, Action::Radius(1)),
            (KeyCode::KeyR, Action::Tool(Tool::Raise)),
            (KeyCode::KeyF, Action::Tool(Tool::Lower)),
            (KeyCode::KeyT, Action::Tool(Tool::Flatten)),
            (KeyCode::KeyG, Action::Tool(Tool::Smooth)),
            (KeyCode::KeyP, Action::Tool(Tool::Paint)),
            (KeyCode::KeyV, Action::NextSurface),
            (KeyCode::KeyH, Action::Falloff),
        ]
    };
    let pressed: Vec<_> = bindings
        .iter()
        .filter(|(key, _)| keys.just_pressed(*key))
        .copied()
        .collect();
    if pressed.is_empty() {
        return;
    }
    let mut keys = world.resource_mut::<ButtonInput<KeyCode>>();
    for (key, _) in &pressed {
        keys.reset(*key);
    }
    world
        .resource_mut::<ShapeTools>()
        .requests
        .extend(pressed.into_iter().map(|(_, action)| action));
}

fn finish_stroke(world: &mut World) {
    let new = {
        let mut state = world.resource_mut::<ShapeTools>();
        state.stroke.take().is_some_and(|stroke| {
            let new = !stroke.cells.is_empty();
            state.history.push(stroke.cells);
            new
        })
    };
    // A new stroke starts a new future for placements too.
    if new && let Some(mut placing) = world.get_resource_mut::<placing::Placing>() {
        placing.forget_redo();
    }
}

/// A placing or resource tool is in hand: the terrain brush rests.
pub(crate) fn other_tool(world: &World) -> bool {
    resources::active(world) || placing::active(world)
}

pub(crate) fn pointer(world: &mut World) {
    // A resource tool in hand: the terrain brush rests.
    if !editing(world) || other_tool(world) {
        finish_stroke(world);
        let mut state = world.resource_mut::<ShapeTools>();
        state.hover = None;
        state.preview_error = None;
        state.preview_key = None;
        state.outline.clear();
        return;
    }
    let ui_hovered = world
        .query_filtered::<&Hovered, With<Node>>()
        .iter(world)
        .any(Hovered::get);
    if ui_hovered {
        finish_stroke(world);
        let mut state = world.resource_mut::<ShapeTools>();
        state.hover = None;
        state.outline.clear();
        return;
    }
    let cell =
        picking::cursor_cell(world).filter(|(x, z)| picking::terrain_cell(world, *x, *z).is_some());
    let (pressed, held) =
        world
            .get_resource::<ButtonInput<MouseButton>>()
            .map_or((false, false), |mouse| {
                (
                    mouse.just_pressed(MouseButton::Left),
                    mouse.pressed(MouseButton::Left),
                )
            });
    world.resource_mut::<ShapeTools>().hover = cell;
    if pressed && let Some((x, z)) = cell {
        finish_stroke(world);
        let anchor_mm = {
            let state = world.resource::<ShapeTools>();
            sample(world, &state.expected, x, z).map_or(0, |cell| cell.height_mm)
        };
        world.resource_mut::<ShapeTools>().stroke = Some(ActiveStroke {
            cells: Stroke::default(),
            anchor_mm,
            last_cell: None,
            since_dab: DAB_SECS,
            commands: 0,
        });
    }
    if !held && !pressed {
        finish_stroke(world);
    }
    let delta = world.get_resource::<Time>().map_or(0.0, Time::delta_secs);
    let due = world
        .resource_mut::<ShapeTools>()
        .stroke
        .as_mut()
        .is_some_and(|stroke| {
            stroke.since_dab += delta;
            stroke.since_dab >= DAB_SECS
        });
    if due && let Some(cell) = cell {
        dab(world, cell);
    }
    refresh_preview(world, cell);
    let outline = cell.map_or_else(Vec::new, |center| {
        let state = world.resource::<ShapeTools>();
        let radius = state.brush.radius.min(picking::limits(world).max_radius);
        model::disc(center, radius)
            .filter_map(|(x, z)| {
                let cell = sample(world, &state.expected, x, z)?;
                let y = if cell.surface == TerrainSurface::Water {
                    -0.10
                } else {
                    cell.height_mm as f32 / 1000.0 + 0.04
                };
                Some(Vec3::new(x as f32 + 0.5, y, z as f32 + 0.5))
            })
            .collect()
    });
    world.resource_mut::<ShapeTools>().outline = outline;
    // Painting must never also select a villager or issue a movement order.
    // `pressed` is kept so the drag continues and its release is still seen.
    if let Some(mut mouse) = world.get_resource_mut::<ButtonInput<MouseButton>>() {
        for button in [MouseButton::Left, MouseButton::Right, MouseButton::Middle] {
            mouse.clear_just_pressed(button);
            mouse.clear_just_released(button);
        }
    }
    if let Some(mut drag) = world.get_resource_mut::<DragBox>() {
        drag.start = None;
    }
}

/// One rate-limited dab: plan from current (or already planned) heights,
/// then queue its one absolute command (a disc or a batch of cells).
fn dab(world: &mut World, cell: (i32, i32)) {
    let (brush, anchor_mm, moved, waited) = {
        let state = world.resource::<ShapeTools>();
        let Some(stroke) = state.stroke.as_ref() else {
            return;
        };
        (
            state.brush,
            stroke.anchor_mm,
            stroke.last_cell != Some(cell),
            stroke.since_dab,
        )
    };
    if !moved && !(brush.tool.repeats() && waited >= REPEAT_SECS) {
        return;
    }
    let limits = limits(world);
    let expected = std::mem::take(&mut world.resource_mut::<ShapeTools>().expected);
    let edits = model::plan_dab(&brush, cell, anchor_mm, &limits, |x, z| {
        sample(world, &expected, x, z)
    });
    let commands = model::batch(
        &edits,
        cell,
        brush.radius.min(limits.max_radius),
        limits.max_batch,
        |x, z| sample(world, &expected, x, z),
    );
    let mut state = world.resource_mut::<ShapeTools>();
    state.expected = expected;
    let queued = state.outbox.len();
    let stroke = state.stroke.as_mut().expect("active stroke");
    stroke.last_cell = Some(cell);
    stroke.since_dab = 0.0;
    if commands.is_empty() {
        return;
    }
    // A stroke stays small enough for its undo to be one batch.
    let too_wide = limits.max_batch > 0 && stroke.cells.len_with(&edits) > limits.max_batch;
    if too_wide || stroke.commands + commands.len() > MAX_STROKE_COMMANDS {
        state.status = "This stroke reached its size limit. Release and press again.".into();
        state.latency.dropped += 1;
        return;
    }
    if queued + commands.len() > OUTBOX_LIMIT {
        state.status =
            "Edits are queued faster than the world applies them. Resume or slow down.".into();
        state.latency.dropped += 1;
        return;
    }
    stroke.commands += commands.len();
    stroke.cells.absorb(&edits);
    for edit in &edits {
        state.expected.insert((edit.x, edit.z), edit.after);
    }
    if state.latency.enabled && state.latency.label.is_empty() {
        state.latency.label = format!(
            "{} {} radius {}{}",
            brush.tool.name(),
            if brush.falloff == Falloff::Soft {
                "soft"
            } else {
                "hard"
            },
            brush.radius.min(limits.max_radius),
            if limits.max_batch == 0 {
                ", cell by cell"
            } else {
                ", batched"
            }
        );
    }
    enqueue(&mut state, commands, Origin::Dab, &edits);
}

/// The reason the simulation would refuse the next dab here, if any.
fn refresh_preview(world: &mut World, cell: Option<(i32, i32)>) {
    let Some(cell) = cell else {
        let mut state = world.resource_mut::<ShapeTools>();
        state.preview_error = None;
        state.preview_key = None;
        return;
    };
    let key = {
        let state = world.resource::<ShapeTools>();
        PreviewKey {
            cell,
            brush: state.brush,
            revision: picking::revision(world),
            planned: state.expected.len(),
        }
    };
    if world.resource::<ShapeTools>().preview_key.as_ref() == Some(&key) {
        return;
    }
    if world.resource::<ShapeTools>().stroke.is_some() {
        // While dragging, the simulation's own answer is the feedback; a
        // second full validation per dab would only slow the stroke.
        let mut state = world.resource_mut::<ShapeTools>();
        state.preview_error = None;
        state.preview_key = None;
        return;
    }
    let limits = limits(world);
    let error = {
        let state = world.resource::<ShapeTools>();
        // A new stroke would anchor Flatten to the height under the cursor.
        let anchor_mm =
            sample(world, &state.expected, cell.0, cell.1).map_or(0, |cell| cell.height_mm);
        let read = |x, z| sample(world, &state.expected, x, z);
        let edits = model::plan_dab(&key.brush, cell, anchor_mm, &limits, read);
        // The dab is one command, so the preview is one validation of all of
        // it. (Cell by cell, the first command stands in for the rest.)
        model::batch(
            &edits,
            cell,
            key.brush.radius.min(limits.max_radius),
            limits.max_batch,
            read,
        )
        .first()
        .and_then(|command| check(world, command).err())
    };
    let mut state = world.resource_mut::<ShapeTools>();
    state.preview_error = error;
    state.preview_key = Some(key);
}

fn apply_request(world: &mut World, action: Action) {
    let limits = limits(world);
    // Taking up a terrain tool puts a resource tool down.
    if matches!(
        action,
        Action::Tool(_) | Action::Surface(_) | Action::NextSurface
    ) {
        if let Some(mut resources) = world.get_resource_mut::<resources::Resources>() {
            resources.mode = None;
        }
        if let Some(mut placing) = world.get_resource_mut::<placing::Placing>() {
            placing.put_down();
        }
    }
    match action {
        Action::Tool(tool) => world.resource_mut::<ShapeTools>().brush.tool = tool,
        Action::Surface(surface) => {
            let mut state = world.resource_mut::<ShapeTools>();
            state.brush.surface = surface;
            state.brush.tool = Tool::Paint;
        }
        Action::NextSurface => {
            let mut state = world.resource_mut::<ShapeTools>();
            let at = SURFACES
                .iter()
                .position(|surface| *surface == state.brush.surface)
                .unwrap_or(0);
            state.brush.surface = SURFACES[(at + 1) % SURFACES.len()];
            state.brush.tool = Tool::Paint;
        }
        Action::Radius(delta) => {
            let mut state = world.resource_mut::<ShapeTools>();
            state.brush.radius = state
                .brush
                .radius
                .saturating_add_signed(delta)
                .min(limits.max_radius);
        }
        Action::SetRadius(radius) => {
            world.resource_mut::<ShapeTools>().brush.radius = radius.min(limits.max_radius);
        }
        Action::Edge(falloff) => world.resource_mut::<ShapeTools>().brush.falloff = falloff,
        Action::Strength(step_mm) => world.resource_mut::<ShapeTools>().brush.step_mm = step_mm,
        Action::Falloff => {
            let mut state = world.resource_mut::<ShapeTools>();
            state.brush.falloff = match state.brush.falloff {
                Falloff::Hard => Falloff::Soft,
                Falloff::Soft => Falloff::Hard,
            };
        }
        Action::Undo | Action::Redo => {
            finish_stroke(world);
            if world.resource::<ShapeTools>().queued() > 0 || placing::busy(world) {
                world.resource_mut::<ShapeTools>().status =
                    "Earlier edits are still being applied. Resume the world, then try again."
                        .into();
                return;
            }
            // Strokes and placements share one timeline: the newer of the
            // two is taken back first, the older laid again first.
            if placing::claims(world, action == Action::Undo) {
                placing::step(world, action == Action::Undo);
                return;
            }
            let max_batch = limits.max_batch;
            let mut history = std::mem::replace(
                &mut world.resource_mut::<ShapeTools>().history,
                History::new(HISTORY_LIMIT),
            );
            let read = |x, z| picking::terrain_cell(world, x, z);
            let step = if action == Action::Undo {
                Step::Undo
            } else {
                Step::Redo
            };
            let replay = match step {
                Step::Undo => history.undo(max_batch, read),
                Step::Redo => history.redo(max_batch, read),
            };
            let mut state = world.resource_mut::<ShapeTools>();
            state.history = history;
            match replay {
                None => {
                    state.status = format!(
                        "Nothing to {}.",
                        if step == Step::Undo { "undo" } else { "redo" }
                    );
                }
                Some(replay) => {
                    let cells: usize = replay
                        .commands
                        .iter()
                        .map(|command| command.footprint().len())
                        .sum();
                    state.status = format!(
                        "{} requested · {cells} cells",
                        if step == Step::Undo { "Undo" } else { "Redo" },
                    );
                    state.expected.extend(replay.expect);
                    let origin = Origin::History {
                        stroke: replay.stroke,
                        step,
                        whole: replay.commands.len() == 1,
                    };
                    if state.latency.enabled && state.latency.label.is_empty() {
                        state.latency.label =
                            if step == Step::Undo { "Undo" } else { "Redo" }.into();
                    }
                    enqueue(&mut state, replay.commands, origin, &[]);
                }
            }
        }
    }
}

fn process(world: &mut World) {
    let epoch = npc_framework::agent_io::api::epoch(world);
    match world.resource::<ShapeTools>().epoch {
        Some(known) if known != epoch => {
            // A different world: its heights are not this history's.
            let mut state = world.resource_mut::<ShapeTools>();
            let (brush, per_cell) = (state.brush, state.per_cell);
            let latency = Latency {
                enabled: state.latency.enabled,
                ..default()
            };
            *state = ShapeTools {
                brush,
                per_cell,
                latency,
                epoch: Some(epoch),
                ..default()
            };
        }
        Some(_) => {}
        None => world.resource_mut::<ShapeTools>().epoch = Some(epoch),
    }
    let requests = std::mem::take(&mut world.resource_mut::<ShapeTools>().requests);
    for request in requests {
        apply_request(world, request);
    }
    // Record a bounded number of planned commands per frame.
    for _ in 0..SUBMIT_PER_FRAME {
        let queued = {
            let mut state = world.resource_mut::<ShapeTools>();
            if state.inflight.len() >= MAX_INFLIGHT {
                break;
            }
            let Some(queued) = state.outbox.pop_front() else {
                break;
            };
            queued
        };
        // Deferred validation: the tick that applies the command checks it
        // against the world of that moment, and reports a refusal below.
        match submit_deferred(world, None, sim_command(&queued.command)) {
            Ok(key) => world
                .resource_mut::<ShapeTools>()
                .inflight
                .push_back((key, queued)),
            Err(error) => reject(world, &queued, error.to_string()),
        }
    }
    settle(world);
    measure(world);
    let delta = world.get_resource::<Time>().map_or(0.0, Time::delta_secs);
    let mut state = world.resource_mut::<ShapeTools>();
    if let Some((_, left)) = state.rejection.as_mut() {
        *left -= delta;
    }
    if state
        .rejection
        .as_ref()
        .is_some_and(|(_, left)| *left <= 0.0)
    {
        state.rejection = None;
    }
    if state.queued() == 0 && state.stroke.is_none() {
        state.expected.clear();
    }
}

/// The simulation refused a command: forget what it would have left behind,
/// say why beside the cursor, and take a refused history step back.
fn reject(world: &mut World, queued: &Queued, reason: String) {
    let mut state = world.resource_mut::<ShapeTools>();
    for at in queued.command.footprint() {
        state.expected.remove(&at);
    }
    // A refused dab is no part of its stroke: the stroke's undo and redo
    // then hold only what the world took. The stroke is still being drawn,
    // or it is already the newest in the history.
    if queued.origin == Origin::Dab && !queued.changes.is_empty() {
        let active = state
            .stroke
            .as_mut()
            .is_some_and(|stroke| stroke.cells.retract(&queued.changes));
        if !active {
            state.history.retract(&queued.changes);
        }
    }
    state.refused += 1;
    state.status = match queued.origin {
        Origin::History {
            stroke,
            step,
            whole: true,
        } if state.history.take_back(stroke, step) => format!(
            "{} not applied: {reason}",
            if step == Step::Undo { "Undo" } else { "Redo" }
        ),
        _ => format!("Not applied: {reason}"),
    };
    state.rejection = Some((reason, REJECTION_SECS));
    state.latency.refused += 1;
    answered(&mut state, queued.edit, None);
}

/// One command of `edit` has its answer. An applied one is in the terrain
/// from `revision` on; a refused one never reaches the mesh.
fn answered(state: &mut ShapeTools, edit: u64, revision: Option<u64>) {
    let Some(probe) = state.latency.waiting.get_mut(&edit) else {
        return;
    };
    probe.left = probe.left.saturating_sub(1);
    probe.revision = probe.revision.max(revision);
    if probe.left == 0 && probe.revision.is_none() {
        // Nothing of the edit was applied: there is no mesh update to time.
        state.latency.waiting.remove(&edit);
    }
}

/// Close the timing of every edit the terrain mesh now shows, and log one
/// line per finished stroke or history step.
fn measure(world: &mut World) {
    if !world.resource::<ShapeTools>().latency.enabled {
        return;
    }
    // The regional scene reports the revision its meshes were built from;
    // without it an edit counts as shown once the simulation has applied it.
    let shown = world
        .get_resource::<crate::colony_scene::living_scene::TerrainMeshRevision>()
        .map_or(u64::MAX, |mesh| mesh.0);
    let now = Instant::now();
    let mut state = world.resource_mut::<ShapeTools>();
    let done: Vec<u64> = state
        .latency
        .waiting
        .iter()
        .filter(|(_, probe)| {
            probe.left == 0 && probe.revision.is_some_and(|needed| shown >= needed)
        })
        .map(|(edit, _)| *edit)
        .collect();
    for edit in done {
        let probe = state.latency.waiting.remove(&edit).expect("listed");
        let elapsed = now.duration_since(probe.began).as_secs_f32() * 1000.0;
        state.latency.samples.push(elapsed);
    }
    if state.stroke.is_some() || !state.latency.waiting.is_empty() || state.queued() > 0 {
        return;
    }
    let latency = std::mem::replace(
        &mut state.latency,
        Latency {
            enabled: true,
            ..default()
        },
    );
    state.latency.next = latency.next;
    if latency.label.is_empty() {
        return;
    }
    let (p50, p95, max) = latency_summary(&latency.samples).unwrap_or_default();
    info!(
        "shape stroke: {} | edits shown {} | commands {} | cells {} | refused {} | dabs dropped {} | plan-to-mesh ms p50 {p50:.0} p95 {p95:.0} max {max:.0}",
        latency.label,
        latency.samples.len(),
        latency.commands,
        latency.cells,
        latency.refused,
        latency.dropped,
    );
}

/// Match recorded commands with the simulation's own outcome.
fn settle(world: &mut World) {
    if world.resource::<ShapeTools>().inflight.is_empty() {
        return;
    }
    let pending: Vec<_> = world
        .resource::<ShapeTools>()
        .inflight
        .iter()
        .map(|(key, queued)| {
            let waiting = world
                .get_resource::<CommandQueue>()
                .is_some_and(|queue| queue.pending.contains_key(key));
            (*key, queued.clone(), waiting)
        })
        .collect();
    let revision = picking::revision(world);
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
    let mut applied = 0;
    let mut waiting = VecDeque::new();
    for (key, queued, still_queued) in pending {
        if still_queued {
            waiting.push_back((key, queued));
        } else if let Some(reason) = skipped.get(&key) {
            reject(world, &queued, reason.clone());
        } else {
            applied += 1;
            answered(
                &mut world.resource_mut::<ShapeTools>(),
                queued.edit,
                Some(revision),
            );
        }
    }
    let mut state = world.resource_mut::<ShapeTools>();
    state.inflight = waiting;
    if applied > 0 {
        state.applied += applied;
        if state.queued() == 0 && state.rejection.is_none() {
            state.status = format!("Landscape updated · {} edits applied", state.applied);
        }
    }
}

/// The brush outline's own gizmo group: thicker lines drawn over the
/// ground, so the outline reads on grass as well as on stone.
#[derive(Default, Reflect, GizmoConfigGroup)]
pub(crate) struct ShapeGizmos;

/// Line width of the brush outline, in pixels.
pub(crate) const OUTLINE_WIDTH: f32 = 3.0;

fn outline_style(mut store: ResMut<GizmoConfigStore>) {
    let (config, _) = store.config_mut::<ShapeGizmos>();
    config.line.width = OUTLINE_WIDTH;
    // Drawn in front of the terrain it outlines.
    config.depth_bias = -0.5;
}

fn gizmo(mode: Res<IslandMode>, state: Res<ShapeTools>, mut gizmos: Gizmos<ShapeGizmos>) {
    if *mode != IslandMode::Editor || state.hover.is_none() {
        return;
    }
    if state.cursor_reason().is_none() {
        for center in &state.outline {
            gizmos.cube(
                Transform::from_translation(*center).with_scale(Vec3::new(1.0, 0.06, 1.0)),
                crate::game_ui::GOLD,
            );
        }
        return;
    }
    // Refused: red and dashed, so the refusal is not told by colour alone.
    let color = crate::game_ui::tokens::CRIT_LIGHT;
    for center in &state.outline {
        let corners = [
            Vec3::new(-0.5, 0.03, -0.5),
            Vec3::new(0.5, 0.03, -0.5),
            Vec3::new(0.5, 0.03, 0.5),
            Vec3::new(-0.5, 0.03, 0.5),
        ];
        for (at, from) in corners.iter().enumerate() {
            let to = corners[(at + 1) % corners.len()];
            for (start, end) in model::DASHES {
                gizmos.line(
                    *center + from.lerp(to, start),
                    *center + from.lerp(to, end),
                    color,
                );
            }
        }
    }
}
